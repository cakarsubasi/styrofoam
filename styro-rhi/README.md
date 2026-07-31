# styrofoam render hardware interface

Heavily WIP. Since a big focus is getting the _shape_ of everything right first, there is a fair bit of code missing to implement basic functionality and I need to go through everything to actually clean it up.

Inspired by Sebbi's [No Graphics API](https://www.sebastianaaltonen.com/blog/no-graphics-api) post (and quite a few of his tweets even before that), the goal is to make a rather compact but powerful API that feels natural to use for my needs.

### TODO:

- Wrap remaining Vulkan structures.
- C interface.
- Proper image copies. (Maybe also VK_EXT_host_image_copy?)
- More documentation and a sensible safety boundary. Basically, we want to avoid making everything unsafe but the functions we mark as unsafe should have sensible usage preconditions. In addition, catching all incorrect API usage especially on the device side is out of scope.
- Complete missing parts of the interface. More complete dynamic state management, simplified image layout transitions, correct offset usage and bounds checking for GpuPtr and so on.

To investigate after getting the basics right:
- Mesh shaders
- Ray tracing shaders
- Device generated commands
