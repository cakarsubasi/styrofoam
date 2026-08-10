# styrofoam render hardware interface

Heavily WIP. Since a big focus is getting the _shape_ of everything right first, there is a fair bit of code missing to implement basic functionality and I need to go through everything to actually clean it up.

Inspired by Sebbi's [No Graphics API](https://www.sebastianaaltonen.com/blog/no-graphics-api) post (and quite a few of his tweets even before that), the goal is to make a rather compact but powerful API that feels natural to use for my needs.

### TODO:

- [x] ~~Wrap remaining Vulkan structures.~~
- [ ] C interface. 
- [x] ~~Proper image copies.~~
- [ ] Documentation
- Complete missing parts of the interface. 
  - [x] More complete dynamic state management
  - [ ] simplified image layout transitions
  - [ ] correct offset usage and bounds checking for GpuPtr.
  - [ ] Mesh shaders

To investigate after getting the basics right:
- Ray tracing shaders
- Device generated commands

Shader usage:
- Install Vulkan SDK
- Compile shaders like following:
```bash
slangc -target spirv -fvk-use-entrypoint-name -capability spvDescriptorHeapEXT -capability SPV_EXT_descriptor_heap -capability SPV_KHR_untyped_pointers  -spirv-resource-heap-stride 32 {PATH_TO_SHADER.slang} -o {PATH_TO_OUTPUT_FILE.spv}
```
