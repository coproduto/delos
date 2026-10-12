The current directory contains an empty Rust project. You should make changes to this directory, and **only this directory**, in order to build an image-editing application.

The full spec for the application can be found in SPEC.md. However, the specification might contain ambiguous or underspecified parts.

When any part of the specification is ambiguous, refer to the following principles:

- This application is meant for generative image editing. In the sense that the application's ability to generate and combine types of content is just as important as the ability to load and manipulate existing images.
- Performance is a critical requirement. Most editing operations should be effectively instant from the user's point of view, even when manipulating large images. Manipulating images up to 16K resolution should be commonplace and tested for. If an operation cannot be completed instantly this should be clearly notified to the user and processing indicators should be provided.
- Memory usage must be kept as low as possible. We will be dealing with very large images in a generative manner, so it is preferrable to only generate images when they are needed rather than excessively caching them in memory.
- Delos must support most common web formats. Support for obscure formats or formats that are little-used on the web is not a priority, but it would be desirable to use an architecture which supports being extended with new image formats easily.
- Delos must be essentially unlimited in how many types of nodes it should support. The architecture must always assume that more nodes might be added.
