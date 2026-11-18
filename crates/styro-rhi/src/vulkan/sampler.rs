use std::hash::Hash;

use ash::vk;

use crate::*;

fn quantize(num: f32, min: f32, max: f32, bits: u32) -> u64 {
    assert!(max > min);
    assert!(bits > 0 && bits < 63);

    let num = num.clamp(min, max);

    let max_int = (1u64 << bits) - 1;
    let t = (num - min) / (max - min);

    (t as f64 * max_int as f64).round() as u64
}

impl Hash for SamplerInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.mag_filter.hash(state);
        self.min_filter.hash(state);
        self.mipmap_mode.hash(state);
        self.address_mode.hash(state);
        let anisotropy = quantize(self.anisotropy, 1.0, 16.0, 32);
        anisotropy.hash(state);
        // Are these good values? Who knows!
        let lod_bias = quantize(self.lod_bias, -1e6, 16.0, 32);
        lod_bias.hash(state);
        let lod_range_lo = quantize(self.lod_range[0], -1e6, 1e6, 32);
        let lod_range_hi = quantize(self.lod_range[1], -1e6, 1e6, 32);
        lod_range_lo.hash(state);
        lod_range_hi.hash(state);
        self.compare_op.hash(state);
    }
}

impl Eq for SamplerInfo {}

impl SamplerInfo {
    pub(super) fn to_vk(&self) -> vk::SamplerCreateInfo<'_> {
        let anisotropy_enable = self.anisotropy > 1.0;
        let max_anisotropy = if anisotropy_enable {
            self.anisotropy
        } else {
            0.0
        };
        let compare_enable = self.compare_op.is_some();
        let compare_op = if let Some(compare_op) = self.compare_op {
            compare_op
        } else {
            CompareOp::Never // doesn't matter
        };
        vk::SamplerCreateInfo::default()
            //.flags(vk::SamplerCreateFlags::DESCRIPTOR_BUFFER_CAPTURE_REPLAY_EXT)
            .mag_filter(self.mag_filter.into())
            .min_filter(self.min_filter.into())
            .mipmap_mode(self.mipmap_mode.into())
            .address_mode_u(self.address_mode[0].into())
            .address_mode_v(self.address_mode[1].into())
            .address_mode_w(self.address_mode[2].into())
            .anisotropy_enable(anisotropy_enable)
            .max_anisotropy(max_anisotropy)
            .mip_lod_bias(self.lod_bias)
            .min_lod(self.lod_range[0])
            .max_lod(self.lod_range[1])
            .compare_enable(compare_enable)
            .compare_op(compare_op.into())
            .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
            .unnormalized_coordinates(false)
    }
}
