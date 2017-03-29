var map, box, paths = [];
function initMap() {
    var uluru = {lat: -25.363, lng: 131.044};
    map = new google.maps.Map(document.getElementById('map'), {
        zoom: 4,
        center: uluru
    });

    var bounds = {
        north: -25.63,
        south: -25.83,
        east: 131.044,
        west: 130.944
    };

    // Define a box and set its editable property to true.
    box = new google.maps.Rectangle({
        bounds: bounds,
        editable: true,
    });
    box.setMap(map);

    $('#locmenu').val('houston');
    panToLocation(curLoc);
}
