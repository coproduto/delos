| Clause | Status | graph | nodes | engine | formats | worker | expr | project | ui/state | ui/session | ui/views | build | test code | tests |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| D1 Generative image editing | Partial |  | ● | • |  |  |  |  |  |  | • |  |  | 2 |
| R1 Written in Rust | Implemented |  |  | ● |  |  |  |  |  |  |  | • |  |  |
| R2 Desktop, web and mobile with Dioxus | Partial |  |  |  |  | • |  |  |  |  | ● | • |  |  |
| R3 GPU acceleration wherever possible | Partial |  | ● | ● |  |  |  |  |  |  |  |  |  | 4 |
| R4 Graceful CPU fallback | Implemented |  |  | ● |  |  |  |  |  |  | • |  |  | 4 |
| F1 Load and display images as graph nodes | Implemented | ● | • | ● | • | • |  |  |  | • | ● |  |  | 3 |
| F1.1 Import creates a Static Image connected to the output | Implemented |  | • |  |  |  |  |  | • |  | ● |  |  | 1 |
| F1.2 Operator parameters: WYSIWYG or form | Implemented |  | ● |  |  |  |  |  | • |  | ● |  |  |  |
| F2 The document is a DAG | Implemented | ● |  | • |  |  |  |  |  |  | • |  |  | 2 |
| F2.1 The image view shows what goes to the output | Partial |  |  |  |  | • |  |  | • |  | ● |  |  | 1 |
| F3 Graph view, alone or beside the image view | Implemented |  |  |  |  |  |  |  | • |  | ● |  |  |  |
| F3.1 Relocating the output | Implemented |  |  |  |  |  |  |  | • |  | ● |  |  | 3 |
| F3.1.1 Double-click to relocate and to reset | Implemented |  |  |  |  |  |  |  |  |  | • |  |  | 1 |
| F3.2 Selecting a node previews it with its controls | Implemented |  |  |  |  |  |  |  | • |  | ● |  |  | 1 |
| F3.2.1 Some operators are edited on their input | Implemented |  | • |  |  |  |  |  | • |  | • |  |  | 1 |
| F3.3 Keep the view fixed while editing upstream | Implemented |  |  |  |  |  |  |  | • |  | ● |  |  | 1 |
| F3.3.1 Controls follow geometry through intermediate nodes | Partial | ● | ● | • |  |  |  |  |  |  |  |  |  | 2 |
| F3.3.2 Say so when the selected node does not contribute | Implemented |  |  |  |  |  |  |  | • |  | • |  |  | 1 |
| F4 Export any operator's output; export the visible image by default | Partial |  |  |  |  |  |  |  | • |  | • |  |  | 1 |
| F4.1 Export follows the relocated output | Implemented |  |  |  |  | • |  |  | • |  |  |  |  | 1 |
| F4.2 Banded, bounded, cancellable export and its dialog | Partial |  |  | • | ● | ● |  |  | • |  | ● |  |  | 6 |
| F5 Combination through operators, with display modalities | Partial | ● | ● |  |  |  |  |  |  |  | ● |  |  | 2 |
| O0 The basic operator set | Implemented |  | • |  |  |  |  |  |  |  | • |  |  | 4 |
| O1 Stack: over-compositing of a front/back chain | Implemented | ● | ● | • |  |  |  |  |  |  |  |  |  | 3 |
| O1.1 Editing a stack: disable, remove | Implemented | ● |  |  |  |  |  |  |  |  | ● |  |  | 1 |
| O2 Disable | Implemented |  | • |  |  |  |  |  |  |  |  |  |  | 1 |
| O3 Simple reflection | Implemented |  | ● | ● |  |  |  |  |  |  | ● |  |  | 2 |
| O4 Downscale | Partial |  | ● | ● |  |  |  |  |  |  |  |  |  |  |
| O5 Background removal with rembg-rs | Partial |  | ● |  |  |  |  |  |  |  |  | • |  |  |
| O6 Invert | Implemented |  | • | • |  |  |  |  |  |  |  |  |  | 2 |
| O7 Clip | Implemented |  | ● |  |  |  |  |  |  |  |  |  |  | 1 |
| O8 Resize with selectable interpolation | Partial |  | ● | • |  |  |  |  |  |  |  |  |  | 1 |
| O9 Blend with the Wikipedia blend modes | Implemented |  | • | ● |  |  |  |  |  |  |  |  |  | 2 |
| O10 Logic-gate blend modes | Implemented |  |  | ● |  |  |  |  |  |  |  |  |  | 1 |
| S1 Splits produce two complementary outputs | Implemented |  | ● | ● |  |  |  |  |  |  |  |  |  | 2 |
| S2 Vertical, horizontal and rectangle splits and their controls | Implemented |  | ● |  |  |  |  |  |  |  | ● |  |  | 1 |
| S3 Exact partition by pixel centre | Implemented |  | • | • |  |  |  |  |  |  |  |  |  | 4 |
| S4 Editing view of a split | Implemented |  | • |  |  |  |  |  |  |  |  |  |  | 1 |
| S5 Specific outputs as inputs, relocation and export targets | Partial | ● |  |  |  |  |  |  | • |  | ● |  |  | 3 |
| S6 Split sizes | Implemented |  | • |  |  |  |  |  |  |  |  |  |  | 1 |
| S7 Any number of outputs per node | Implemented | • | • |  |  |  | • |  |  |  |  |  |  | 2 |
| N1 Generators take no inputs | Implemented |  | ● |  |  |  |  |  |  |  |  |  |  | 1 |
| N2 Size-less generators and the output canvas | Implemented | • | • | • |  |  |  |  |  |  | ● |  |  | 1 |
| N3 Default canvas fits the window | Implemented |  |  |  |  |  |  |  | ● |  | • |  |  |  |
| N4 Generators adapt to what they are combined with | Implemented | • | • |  |  |  |  |  |  |  |  |  |  | 1 |
| N5 The initial generators | Implemented |  | ● | ● |  |  |  |  |  |  |  |  |  | 1 |
| N6 AI image generation (planned) | Deferred |  |  | • |  |  |  |  |  |  |  |  |  |  |
| Z1 Ask which size to use, insert a Resize | Implemented | ● |  |  |  |  |  |  | • |  | ● |  |  | 1 |
| P1 Two ways of working; quick mode | Implemented |  |  |  |  | • |  |  |  | • |  |  |  | 1 |
| P2 Project mode | Implemented |  |  |  |  |  |  | • |  |  |  |  |  | 2 |
| P3 First import asks whether to start a project | Implemented |  |  |  |  |  |  |  |  | • | ● |  |  | 2 |
| P3.1 The question mentions the storage cost | Implemented |  |  |  |  |  |  |  |  |  | • |  |  |  |
| P3.2 Declining starts quick mode for good | Implemented |  |  |  |  |  |  |  |  | • |  |  |  | 1 |
| P3.3 Accepting creates a named project in the default location | Implemented |  |  |  |  | • |  | • |  | ● | • |  |  | 3 |
| P4 New project from current work | Implemented |  |  |  |  |  |  | • |  | • | • |  |  | 1 |
| P5 The footer shows mode, location and save state | Implemented |  |  |  |  | • |  |  |  |  | ● |  |  | 2 |
| P6 Autosave after every change | Implemented |  |  |  |  | • |  |  |  | • |  |  |  | 1 |
| P6.1 Atomic saves | Implemented |  |  |  |  |  |  | ● |  |  |  |  |  |  |
| P6.2 Opening a project writes nothing | Partial |  |  |  |  |  |  |  | ● | • | • |  |  | 1 |
| P7 Confirm before leaving quick mode with work | Partial |  |  |  |  |  |  |  |  | • | ● |  |  | 1 |
| P8 A project is a directory with project.delos and inputs/ | Implemented |  |  |  |  |  |  | • |  |  |  |  |  | 1 |
| P9 Images copied unchanged, or as PNG | Implemented |  |  |  |  | • |  | ● |  |  |  |  |  | 3 |
| P10 inputs/ holds exactly the images in use | Implemented |  |  |  |  | • |  | ● |  |  |  |  |  | 2 |
| P11 Undo history is not part of the project | Implemented |  |  |  |  |  | • |  | • |  |  |  |  | 1 |
| P11.1 Images the history needs live in a session stash | Implemented |  |  |  |  | • |  | • |  |  |  |  |  | 1 |
| P11.2 Undo copies images back unchanged | Implemented |  |  |  |  |  |  | • |  |  |  |  |  | 1 |
| P12 Share Project writes a .delosproject zip | Implemented |  |  |  |  | • |  | • |  |  | • |  |  | 2 |
| P13 Import Project accepts three forms | Implemented |  |  |  |  |  |  | • |  | • | • |  |  | 2 |
| P13.1 Archives unpack under a new name | Implemented |  |  |  |  |  |  | ● |  | • |  |  |  | 2 |
| P13.2 Directories open in place | Implemented |  |  |  |  |  |  | • |  |  |  |  |  | 1 |
| P13.3 Opening by drag and drop | Implemented |  |  |  |  |  |  |  |  |  | • |  |  |  |
| P14 Verify the structure before opening | Implemented |  |  |  |  |  |  | ● |  | • |  |  |  | 2 |
| P15 Unsafe paths are rejected; failed imports leave nothing | Partial |  |  |  |  |  |  | ● |  |  |  |  |  | 2 |
| X1 Mobile: projects without pickers | Implemented |  |  |  |  |  |  |  |  |  | ● | • |  |  |
| X2 Web: always quick mode | Implemented |  |  |  |  |  |  |  |  | • | • | • |  |  |
| X3 Platform limits accepted for now | Deferred |  |  |  |  |  |  |  |  |  |  |  |  |  |
| E1 A Lisp-like plain-text form of the graph | Implemented |  |  |  |  |  | ● | • |  |  |  |  |  | 2 |
| E2 Nodes, inputs in port order, `_` for empty ports | Implemented |  |  |  |  |  | ● |  |  |  |  |  |  | 2 |
| E3 Several outputs: `def` and `name.output` | Implemented |  |  |  |  |  | ● |  |  |  |  |  |  | 4 |
| E4 `(stack front … back)` | Implemented |  |  |  |  |  | ● |  |  |  |  |  |  | 1 |
| E5 `(image "inputs/…")` | Implemented |  |  |  |  |  | ● |  |  |  |  |  |  | 2 |
| E6 `:at` and `:title` | Implemented |  |  |  |  |  | • |  |  |  |  |  |  | 1 |
| E7 Values and comments | Implemented |  |  |  |  |  | ● |  |  |  |  |  |  | 2 |
| E8 Write every parameter; read with defaults and auto-layout | Implemented | • |  |  |  |  | ● |  |  |  |  |  |  | 2 |
| E9 Fast serialisation | Implemented |  |  |  |  |  | • | • |  |  |  |  |  | 1 |
| E10 Round trip gives a functionally identical graph | Implemented |  |  |  |  |  | • |  |  |  |  |  |  | 4 |
| E11 Errors with line and column | Partial |  |  |  |  |  | ● | • |  |  |  |  |  | 3 |
| Q1 Regression tests for fixed bugs | Process |  |  |  |  |  |  |  |  |  |  |  | • | 5 |
| Q2 Projects: unit and end-to-end tests on the real services | Process |  |  |  |  |  |  |  |  | • | • |  | ● | 2 |
| Q3 16K performance tests | Process |  |  |  |  |  |  |  |  |  |  |  | ● | 3 |

• cites fewer than 20 lines of that component, ● 20 or more.
