//! Headless wgpu device, Vello renderer and pixel readback.

use anyhow::{Context, Result};
use vello::util::RenderContext;
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

pub const W: u32 = 1080;
pub const H: u32 = 1920;

pub struct Gpu {
    pub cx: RenderContext,
    pub renderer: Renderer,
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub buffer: wgpu::Buffer,
    pub adapter_info: String,
    pub padded_row: u32,
}

impl Gpu {
    /// A fresh instance, device, renderer and target.
    pub fn new() -> Result<Self> {
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).context("no adapter")?;
        let handle = &cx.devices[id];
        let info = handle.adapter().get_info();
        let adapter_info = format!("{:?}", info);
        let device = &handle.device;
        let renderer = Renderer::new(
            device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..RendererOptions::default()
            },
        )
        .map_err(|e| anyhow::anyhow!("renderer: {e:?}"))?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let padded_row = (W * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (padded_row * H) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            cx,
            renderer,
            texture,
            view,
            buffer,
            adapter_info,
            padded_row,
        })
    }

    fn handle(&self) -> &vello::util::DeviceHandle {
        &self.cx.devices[0]
    }

    /// Renders the scene into the target texture (asynchronous on the GPU).
    pub fn render(&mut self, scene: &Scene) -> Result<()> {
        let h = &self.cx.devices[0];
        self.renderer
            .render_to_texture(
                &h.device,
                &h.queue,
                scene,
                &self.view,
                &RenderParams {
                    base_color: vello::peniko::color::palette::css::BLACK,
                    width: W,
                    height: H,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| anyhow::anyhow!("render: {e:?}"))?;
        Ok(())
    }

    /// Waits for submitted work (no readback).
    pub fn wait(&self) {
        let _ = self
            .handle()
            .device
            .poll(wgpu::PollType::wait_indefinitely());
    }

    /// Copies the texture to the CPU as tightly packed RGBA8.
    pub fn read_pixels(&self, out: &mut Vec<u8>) -> Result<()> {
        let h = self.handle();
        let mut enc = h
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        enc.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row),
                    rows_per_image: Some(H),
                },
            },
            wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
        );
        h.queue.submit([enc.finish()]);
        let slice = self.buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = h.device.poll(wgpu::PollType::wait_indefinitely());
        rx.recv()??;
        {
            let data = slice
                .get_mapped_range()
                .map_err(|e| anyhow::anyhow!("map: {e:?}"))?;
            out.clear();
            out.reserve((W * 4 * H) as usize);
            for row in 0..H as usize {
                let s = row * self.padded_row as usize;
                out.extend_from_slice(&data[s..s + (W * 4) as usize]);
            }
        }
        self.buffer.unmap();
        Ok(())
    }
}
