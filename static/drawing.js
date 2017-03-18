// http://www.williammalone.com/articles/create-html5-canvas-javascript-drawing-app/#demo-simple

var canvas = document.getElementById('shape');
// Get width and height from CSS, slice off px measurement.
canvas.setAttribute('width', getComputedStyle(canvas).width.slice(0, -2));
canvas.setAttribute('height', getComputedStyle(canvas).height.slice(0, -2));
var context = canvas.getContext('2d');
var clickX = [];
var clickY = [];
var clickDrag = [];
var paint = false;

$('#shape').mousedown(function(e){
   // x,y retreival from comment section in
    // http://www.html5canvastutorials.com/advanced/html5-canvas-mouse-coordinates/
    var rect = canvas.getBoundingClientRect();
    var x = (e.clientX-rect.left)/(rect.right-rect.left)*canvas.width;
    var y = (e.clientY-rect.top)/(rect.bottom-rect.top)*canvas.height;

    paint = true;
    addClick(x, y, false);
    redraw();
});

$('#shape').mousemove(function(e){
    if(paint){
        var rect = canvas.getBoundingClientRect();
        var x = (e.clientX-rect.left)/(rect.right-rect.left)*canvas.width;
        var y = (e.clientY-rect.top)/(rect.bottom-rect.top)*canvas.height;
        addClick(x, y, true);
        redraw();
    }
});

$('#shape').mouseup(function(e){
    paint = false;
});

$('#shape').mouseleave(function(e){
    paint = false;
});


function addClick(x, y, dragging)
{
    clickX.push(x);
    clickY.push(y);
    clickDrag.push(dragging);
}

function redraw() {
    context.clearRect(0, 0, canvas.width, canvas.height); // Clears the canvas

    context.strokeStyle = "#df4b26";
    context.lineJoin = "round";
    context.lineWidth = 1;

    for(var i=0; i < clickX.length; i++) {
        context.beginPath();
        if(clickDrag[i] && i){
            context.moveTo(clickX[i-1], clickY[i-1]);
        }else{
            context.moveTo(clickX[i]-1, clickY[i]);
        }
        context.lineTo(clickX[i], clickY[i]);
        context.closePath();
        context.stroke();
    }
}

function clearShape() {
    clickX = [];
    clickY = [];
    clickDrag = [];
    paint = false;
    redraw();
}

function getShape() {
    // Return matrix where 1 = traced pixel, 0 = not set.
    var d = context.getImageData(0, 0, canvas.width, canvas.height);
    var dd = d.data;
    var s = [];
    for (var i = 0; i < d.height; i++) {
        var c = [];
        s.push(c);
        for (var j = 0; j < d.width; j++) {
            var k = 4 * (i * d.width + j);
            if (dd[k] > 0 || dd[k+1] > 0 || dd[k+2] > 0 || dd[k+3] > 0) {
                c.push(true);
            } else {
                c.push(false);
            }
        }
    }
    return s;
}

$('#clearshape').on('click', clearShape);

