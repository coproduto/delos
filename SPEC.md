# Delos - a generative image editing tool

Delos is to be an image editing tool focused on combining existing
content with generative content. By generative content, we mean both
AI-generated content and traditionally generative content, using
classical procedural generation algorithms.

## Requirements

Delos must be built using the Rust programming language. 

Delos must be possible to run as a desktop application, a 
web-based application and a mobile application. We would like to use
Dioxus as our framework for this purpose.

Delos will extensively display and manipulate images. Using GPU acceleration wherever possible is a requirement for this project. However, in case GPU acceleration is not available, Delos should degrade gracefully to CPU rendering. 

## Basic functionalities

1. Delos must be able to load and display existing images. Opening an image creates a "layer" containing that image. Layers are treated as graph nodes which can be connected using "operators". 
   - 1.1. Importing an image to Delos specifically creates a "Static Image" node connected directly to the output.
   - 1.2. Operators may have parameters. In case they do, adding the operator node will allow  the user to input the operator parameters either via a WYSIWYG interface or via form inputs.
2. While an image is being edited in Delos, we will be able to visualize the final results. During the process of editing, however, the image will be stored as a Directed Acyclic Graph (DAG) representing image layers and operations applied to them. That way, we can easily see and modify effects are being applied to which parts of the image.
   - 2.1. The image view must show the image that is being sent to the output.
3. We must be able to open a graph view instead of the image view or alongside the image view. It must also be possible to visualize any part of the graph which produces an image.
   - 3.1. In other terms, we can temporarily relocate the output to any image node. 
   - 3.2. If we select an operator node, the image view should show the image at the operator's output overlaid with controls for the specific operator (examples of controls: selection boxes, rules). Examples are given in the operators section below.
4. It must be possible to export the image output by any Delos image operator, but by default the easiest operation to perform should be to export the output currently visible in image view.
   - 4.1. While the output is relocated, exporting the current image will export the content of the node that is currently attached to output.
5. All combinations of images will be done using operators. For example, rather than stacking layers by default, Delos should make it possible to combine layers with specific operators. Delos shall define specific "display modalities" for standard operators to make user experience more pleasurable - for example, if we combine a sequence of layers using the "stack" operator, we can display this as a layer stack rather than as a degenerate graph of the format `(layer3 STACK (layer2 STACK layer1))` (i.e. a Lisp list of stacks, with the exception that it will never be empty hence a NIL-node is not required.)

## Basic Operators

These are basic layer operators which must be implemented in the initial version of Delos:
  * Stack operator - this is a basic operator which is used to form layer stacks. A layer stack will be internally represented as `(frontmost STACK (second-frontmost STACK (third-frontmost STACK ... (second-backmost STACK last) ... )))`. A layer stack will show the first non-transparent pixel found along the stack for each position. Takes no parameters. If the stack operator is selected it should allow editing the stack, for example temporarily disabling inputs (which will be internally represented as a "Disable" operator which changes all pixels to transparent) or removing them completely.
  * Disable operator - mentioned with stack above.
  * Simple reflection operator - this should allow making reflections, i.e. placing axes of reflection on the image which will copy the pixels to one side of them to the other side. This tool, when selected, will allow (via WYSIWYG rule or form input) selecting where to position the axis of reflection. Via form, it should allow selecting a vertical or horizontal axis of reflection. It should also allow specifying whether the reflection should be additive, subtractive or replace the pixels where the reflection will fall.
  * Downscale operator - should allow for downscaling 
  * Background removal operator - this should allow removing the input image's background using the `rembg-rs` crate. Controls set the background removal parameters.
  * Invert operator - inverts colors. (i.e. (R,G,B) -> (1-R,1-G,1-B)) with 0 <=  R, G, B <= 1. No controls.
  * Clip - allows selecting a subset of image.
  * Resize - allows upscaling or downscaling input. Allows choosing upscale/downscale algorithm to use. Should offer nearest-neighbor, bilinear, bicubic and fourier-based interpolation.
  * Blend operator - allows combining images with the blend modes specified at https://en.wikipedia.org/wiki/Blend_modes plus the following:
    * Logic Gate: applies logic operation (and, or, xor) to both images. Note that nand, nor and xnor are not included bc they can be achieved with the invert operator.
  

## Generative operators

These operators are differentiated by not taking any inputs. Rather, they should act as generators of data which can be input to other generators.

Note that generative operators do not have output size - if they are connected directly to output, the user must specify the desired output size (this should be possible to do  via WYSIWYG). The output size  which fits naturally into the current Delos window should be selected by default. If generative operators are connected to combining operators, they will generate at whatever size is necessary to combine successfully with the other side.

## Handling image size differences

As said above, generative operators should generate at the size required of them  and should not specify an output size. However, the outputs produced by imported images and by combining operators will have a definite size. In this case, Delos should ask which output size must be used and insert a resizing node after one of the inputs as appropriate.
