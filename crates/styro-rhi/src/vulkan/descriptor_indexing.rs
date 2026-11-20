use ash::vk;
use ash::vk::TaggedStructure as _;

pub(super) struct DescriptorIndexingData {
    descriptor_pool: vk::DescriptorPool,
    pub descriptor_sets: [vk::DescriptorSet; 3],
    descriptor_set_layouts: [vk::DescriptorSetLayout; 3],
    pub pipeline_layout: vk::PipelineLayout,
}

impl DescriptorIndexingData {
    pub(super) fn new(device: &ash::Device) -> Self {
        unsafe {
            let binding_size = 4096;

            let all_stages = vk::ShaderStageFlags::VERTEX
                | vk::ShaderStageFlags::FRAGMENT
                | vk::ShaderStageFlags::COMPUTE;

            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(3)
                        .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
                        .pool_sizes(&[
                            vk::DescriptorPoolSize::default()
                                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                                .descriptor_count(binding_size),
                            vk::DescriptorPoolSize::default()
                                .ty(vk::DescriptorType::SAMPLER)
                                .descriptor_count(binding_size),
                            vk::DescriptorPoolSize::default()
                                .ty(vk::DescriptorType::STORAGE_IMAGE)
                                .descriptor_count(binding_size),
                        ]),
                    None,
                )
                .unwrap();

            let push_constant_ranges = [vk::PushConstantRange::default()
                .offset(0)
                .size(256)
                .stage_flags(all_stages)];

            let binding_flags = vk::DescriptorBindingFlags::PARTIALLY_BOUND
                | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
                | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING
                | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT;
            let binding_flags = [binding_flags];

            let sampled_image_bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_count(binding_size)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .stage_flags(all_stages)];
            let sampler_bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_count(binding_size)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .stage_flags(all_stages)];
            let storage_image_bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_count(binding_size)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(all_stages)];

            let mut binding_flags_create_info =
                vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                    .binding_flags(&binding_flags);

            let sampled_image_descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&sampled_image_bindings)
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .extend(&mut binding_flags_create_info),
                    None,
                )
                .unwrap();
            let storage_image_descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&storage_image_bindings)
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .extend(&mut binding_flags_create_info),
                    None,
                )
                .unwrap();

            let sampler_descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&sampler_bindings)
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .extend(&mut binding_flags_create_info),
                    None,
                )
                .unwrap();

            let layouts = [
                sampled_image_descriptor_set_layout,
                storage_image_descriptor_set_layout,
                sampler_descriptor_set_layout,
            ];

            let binding_sizes = [binding_size; 3];
            let mut allocate_info = vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
                .descriptor_counts(&binding_sizes);
            let descriptor_sets = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&layouts)
                        .extend(&mut allocate_info),
                )
                .unwrap();
            let descriptor_sets = *descriptor_sets.as_array::<3>().unwrap();

            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .push_constant_ranges(&push_constant_ranges)
                        .set_layouts(&layouts),
                    None,
                )
                .unwrap();

            Self {
                descriptor_pool,
                descriptor_sets,
                descriptor_set_layouts: layouts,
                pipeline_layout,
            }
        }
    }

    pub(super) fn get_sampled_image_set(&self) -> vk::DescriptorSet {
        self.descriptor_sets[0]
    }

    pub(super) fn get_storage_image_set(&self) -> vk::DescriptorSet {
        self.descriptor_sets[1]
    }

    pub(super) fn get_sampler_set(&self) -> vk::DescriptorSet {
        self.descriptor_sets[2]
    }

    pub(super) fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            // Destroying the pool implicitly frees the descriptor sets allocated from it
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            for layout in self.descriptor_set_layouts {
                device.destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}
