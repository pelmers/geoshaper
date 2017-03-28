var results = [];
var resultsMeta = {};
var curLoc = "houston";

$("#locbutton").on('click', function(e) {
    curLoc = $('#loctext').val().trim();
    panToLocation(curLoc);
});

if ($('#loctext').val() === "") {
    $('#loctext').val(curLoc);
} else {
    curLoc = $('#loctext').val().trim();
}
panToLocation(curLoc);

function panToLocation(loc) {
    $("#spinner").show();
    $.get("/bounds/" + loc, function(bounds) {
        console.log("Bounds", bounds);
        map.fitBounds(bounds);
        box.setBounds(bounds);
    }).fail(function() {
        // TODO: offer to download from https://s3.amazonaws.com/metro-extracts.mapzen.com/
        alert("location not found. ?!");
    }).always(function() {
        $("#spinner").hide();
    });
}

$("#results").on('click', function(e) {
    if (!e.target.id.startsWith("result_")) {
        return;
    }
    const d = parseInt(e.target.id.slice("result_".length));
    if (d >= 0 && d < results.length) {
        pathFromLoc(results[d][0], results[d][1]);
    }
});


/**
 * Trim l columns from left, r columns from right, t rows from top, and b rows
 * from bottom of M and return as a new matrix. Does not modify M.
 */
function trimBorder(M, l, r, t, b) {
    var ret = [];
    M.slice(t, M.length - b).forEach(function(row) {
        ret.push(row.slice(l, row.length - r));
    })
    return ret;
}

// Find offset of first true value from the top-left.
function findOffset(matrix) {
    let row = matrix.length,
        col = matrix[0].length;
    for (let i = 0; i < matrix.length; i++) {
        for (let j = 0; j < matrix[0].length; j++) {
            if (matrix[i][j]) {
                if (i < row) {
                    row = i;
                }
                if (j < col) {
                    col = j;
                }
            }
        }
    }
    return {row, col};
}

function autocrop(matrix) {
    // Find the top, bottom, left, right-most true cells
    let bottom = 0,
        top = matrix.length,
        left = matrix[0].length,
        right = 0,
        height = matrix.length,
        width = matrix[0].length;
    for (let i = 0; i < matrix.length; i++) {
        for (let j = 0; j < matrix[0].length; j++) {
            if (matrix[i][j]) {
                if (i > bottom) {
                    bottom = i;
                }
                if (i < top) {
                    top = i;
                }
                if (j > right) {
                    right = j;
                }
                if (j < left) {
                    left = j;
                }
            }
        }
    }
    return trimBorder(matrix, left, width-right-1, top, height-bottom-1);
}

function flip(matrix) {
    var n = [];
    for (var i = matrix.length - 1; i >= 0; i--) {
        var row = matrix[i].concat();
        row.reverse();
        n.push(row);
    }
    return n;
}

$('#clearpaths').on('click', function() {
    for (let i = 0; i < paths.length; i++) {
        paths[i].setMap(null);
    }
    paths = [];
});

var did_snap = false;
$('#snap').on('click', function(e) {
    if (did_snap) {
        return;
    } else {
        did_snap = true;
    }
    let to_snap = paths[paths.length - 1];
    let path = to_snap.getPath();
    let pathValues = [];
    // Google only allows 100 points to snap so skip some if it's too long.
    // TODO: write our own road snap.
    for (var i = 0; i < path.getLength(); i += Math.max(1, path.getLength() / 100)) {
        pathValues.push(path.getAt(Math.floor(i)).toUrlValue());
    }
    // Previous math isn't exact, it may have made 1 extra.
    pathValues = pathValues.slice(0, 100);
    console.log("help me google thanks.", pathValues.length);
    $.get('https://roads.googleapis.com/v1/snapToRoads', {
        //interpolate: true,
        key: 'AIzaSyASZ1-vdBNe0U0XuMkOF4R_GMrHGg2Ah-A',
        path: pathValues.join('|')
    }, function(data) {
        // Remove the old path from the map, and draw the new one.
        to_snap.setMap(null);
        console.log(data);
        if (data.snappedPoints === undefined) {
            return;
        }
        let snappedCoordinates = [];
        for (var i = 0; i < data.snappedPoints.length; i++) {
            var latlng = new google.maps.LatLng(
                data.snappedPoints[i].location.latitude,
                data.snappedPoints[i].location.longitude);
            snappedCoordinates.push(latlng);
        }
        let snapped = new google.maps.Polyline({
            path: snappedCoordinates,
            strokeColor: '#0000FF',
            strokeOpacity: 0.9,
            strokeWeight: 3
        });
        snapped.setMap(map);
        paths.push(snapped);
    }).fail(function(e) {
        console.log("it failed :(");
        console.log(e);
    });
});

function pathFromLoc(bLat, bLon) {
    var sLat = resultsMeta.scale[0];
    var sLon = resultsMeta.scale[1];
    var path = [];
    var offset = findOffset(resultsMeta.shape);
    var shapeCrop = flip(autocrop(resultsMeta.shape));
    for (var i = 0; i < resultsMeta.cX.length; i++) {
        var x = resultsMeta.cX[i] - offset.col;
        var y = shapeCrop[0].length - (resultsMeta.cY[i] - offset.row);
        var pos = {lat: bLat + y*sLat, lng: bLon + x * sLon};
        path.push(pos);
    }
    console.log(path);
    var newPath = new google.maps.Polyline({
        path: path,
        strokeColor: '#FF0000',
        strokeOpacity: 0.9,
        strokeWeight: 3
    });
    newPath.setMap(map);
    paths.push(newPath);
    did_snap = false;
    map.setCenter({lat: bLat, lng: bLon});
}

var doing_it = false;
$('#doit').on('click', function(e) {
    if (doing_it) {
        return;
    } else {
        doing_it = true;
    }
    // Immediately copy shape variables in case user draws in meantime.
    var cX = clickX.slice();
    var cY = clickY.slice();
    var cD = clickDrag.slice();
    var shape = getShape();
    var shapeCrop = flip(autocrop(shape));
    var payload = {
        "shape": shapeCrop,
        "bounds": box.getBounds().toJSON()
    };
    console.log("Firing request!");
    $("#spinner").show();
    $.post("/find_match/" + curLoc, JSON.stringify(payload), function(data) {
        console.log(data);
        results = data.best;
        resultsMeta.scale = data.scale;
        resultsMeta.cX = cX;
        resultsMeta.cY = cY;
        resultsMeta.shape = shape;
        pathFromLoc(results[0][0], results[0][1]);
        let $results = $("#results");
        $results.empty();
        for (var i = 0; i < results.length; i++) {
            let resultsText = `<div id=result_${i}>${results[i][0].toFixed(3)}, ${results[i][1].toFixed(3)}</div>`;
            $results.append(resultsText);
        }
    }).always(function() {
        console.log("Did it finish or did it fail? Really I don't know.");
        doing_it = false;
        $("#spinner").hide();
    });
});
