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
     - 3.1.1. Double-clicking a node in the graph view relocates the output to it. Double-clicking the relocated node again, or the Output node, moves the output back to its default.
   - 3.2. If we select an operator node, the image view should show the image at the operator's output overlaid with controls for the specific operator (examples of controls: selection boxes, rules). Examples are given in the operators section below.
     - 3.2.1. Some operators are edited on their input instead. For example, Clip shows the uncropped input with the crop rectangle.
   - 3.3. It must be possible to keep the image view fixed while editing a node further up the graph. While the output is relocated (or when "Preview selection" is turned off), selecting another node keeps the view as it is and draws that node's controls over the shown image. Editing those controls modifies the shown image directly.
     - 3.3.1. Controls must be placed correctly even when nodes between the edited node and the shown image move or scale pixels (Clip, Resize, Downscale, …). Every node type describes how it maps coordinates to its inputs, so new node types can take part.
     - 3.3.2. If the selected node does not contribute to the shown image, the view says so instead of showing controls.
4. It must be possible to export the image output by any Delos image operator, but by default the easiest operation to perform should be to export the output currently visible in image view.
   - 4.1. While the output is relocated, exporting the current image will export the content of the node that is currently attached to output.
   - 4.2. Export renders at full resolution in bands (to bound memory), shows progress and can be cancelled. The export dialog can be closed with its close button, the Escape key or by clicking outside it, and closes by itself after a successful export.
5. All combinations of images will be done using operators. For example, rather than stacking layers by default, Delos should make it possible to combine layers with specific operators. Delos shall define specific "display modalities" for standard operators to make user experience more pleasurable - for example, if we combine a sequence of layers using the "stack" operator, we can display this as a layer stack rather than as a degenerate graph of the format `(layer3 STACK (layer2 STACK layer1))` (i.e. a Lisp list of stacks, with the exception that it will never be empty hence a NIL-node is not required.)

## Basic Operators

These are basic layer operators which must be implemented in the initial version of Delos:
  * Stack operator - this is a basic operator which is used to form layer stacks. A layer stack will be internally represented as `(frontmost STACK (second-frontmost STACK (third-frontmost STACK ... (second-backmost STACK last) ... )))`. A layer stack will show the first non-transparent pixel found along the stack for each position. Delos implements this as standard "over" alpha compositing: this matches the rule for opaque pixels and also handles partly transparent ones. Takes no parameters. If the stack operator is selected it should allow editing the stack, for example temporarily disabling inputs (which will be internally represented as a "Disable" operator which changes all pixels to transparent) or removing them completely.
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

The initial generators are: solid color, linear gradient, radial gradient, checkerboard, noise (fBm, turbulence, ridged), Voronoi and Mandelbrot. An AI image-generation operator is planned.

## Handling image size differences

As said above, generative operators should generate at the size required of them  and should not specify an output size. However, the outputs produced by imported images and by combining operators will have a definite size. In this case, Delos should ask which output size must be used and insert a resizing node after one of the inputs as appropriate.

## Sessions: quick mode and projects

Delos supports two ways of working:

1. **Quick mode** is for one-off edits and quick work. Imported images are used where they are and nothing is saved.
2. **Project mode** is for longer-term work, and for sharing and collaborating with others.

- When the user imports an image and neither a project nor a quick-mode session is open, Delos asks whether to create a project.
  - The question makes clear that projects copy every imported image, so they take more storage.
  - If the user declines, a quick-mode session starts and later imports are added to it without asking again.
  - If the user accepts, a project is created (named by the user; by default in `~/.delos/projects/<name>`, overridable with `DELOS_PROJECTS_DIR`) and the image is added to it.
- The current work can also be turned into a project at any time ("New project from current work"). The images the graph uses are copied into it.
- The bottom of the window always shows whether Delos is in quick mode or in a project (with its name and location), and whether the project's changes are saved.
- Projects are saved automatically after every change.
  - Saves are atomic, so a project file is never left half-written.
  - Opening a project does not change anything on disk until the user edits it.
- Leaving a quick-mode session that has work in it asks for confirmation.

### Project structure

A project is a directory:

```text
<name>/
  project.delos   the node graph, as a Delos expression (see below)
  inputs/         copies of the images the graph uses
```

- Imported images are copied into `inputs/` unchanged (no re-encoding) when their original bytes are available. Images known only by their pixels are written as PNG.
- `inputs/` contains exactly the images the current graph uses. Images that are no longer used are removed from it.
- **Undo history is not part of a project.** It belongs to the editing session and is not expected to survive closing and reopening a project.
  - Within a session, images the history may still need are kept outside the project (in a temporary session folder).
  - If an undo brings an image back, it is copied back into `inputs/` unchanged.

### Sharing and importing projects

- **Share Project** exports the current project as a `.delosproject` file: a zip archive containing the project directory.
- **Import Project** accepts a `.delosproject` file, a `project.delos` file or a project directory.
  - A `.delosproject` file is unpacked into the projects folder under a new name, so it never overwrites an existing project.
  - A directory is opened in place, without copying.
  - Projects can also be opened by dropping them onto the window.
- Before opening, Delos verifies the structure:
  - `project.delos` exists and parses;
  - `inputs/` exists;
  - every image the graph refers to exists inside `inputs/`.

  It reports precisely what is wrong otherwise.
- Paths that would point outside the project, whether in the project file or inside an archive, are rejected. A failed import leaves nothing behind.

### Delos expressions

The node graph has a plain-text representation, so we can speak of Delos *expressions*. It is a Lisp-like format, in keeping with layer stacks being treated as non-empty lists:

```lisp
; Nodes used more than once are bound with def; everything else nests.
(def photo (image "inputs/photo.png" :at (60 40)))
(output
  (stack
    (invert photo)
    (blend photo (noise :scale 0.3) :mode screen))
  :canvas_w 1920 :canvas_h 1080)
(noise :seed 4)   ; nodes not connected to anything are kept too
```

- A node is written `(kind input… :param value…)`. Inputs come in port order, and `_` leaves a port empty.
- `(stack front … back)` is a layer stack.
- `(image "inputs/…")` is a static image.
- `:at (x y)` is a node's position in the graph view, and `:title "…"` gives it a custom title.
- Values are numbers, `true`/`false`, choices as symbols, colors as `(r g b a)` lists and text as strings. Comments start with `;`.
- When saving, **every parameter is written, including defaults**, so a project keeps its meaning if a default changes later. When reading, omitted parameters take their defaults, and nodes without `:at` are laid out automatically, so expressions can be written by hand.
- Serializing and deserializing must be fast. Doing both must give a functionally identical graph: the same nodes, connections, parameters, titles and positions, rendering the same pixels.
- Errors are reported with a line and column.

### Platform status

Projects are currently desktop-first:

- **Mobile** supports projects but has no file pickers yet, so Import and Share Project are unavailable there.
- **Web** has no file system and always works in quick mode.

These limitations are accepted for now. Full functionality on all platforms is a later goal.

## Quality requirements

- Bugs that are fixed must get regression tests that reproduce them.
- Project features must be covered by unit tests and by end-to-end tests. The end-to-end tests run the application's real state, background worker and autosave without a window, and drive them as the user interface does.
- Performance at 16K resolution is tested: previews must stay interactive, and exports must run in bounded memory.
