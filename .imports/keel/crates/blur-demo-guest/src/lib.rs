//! Native keel guest demoing a wasm **SIMD128** pixel filter.
//!
//! It ships one baseline image filter — a square box blur over straight-alpha
//! RGBA8 — in two forms that MUST agree byte-for-byte:
//!
//! * `box_blur_scalar` — the plain integer reference.
//! * `box_blur_simd`   — hand-vectorized with `core::arch::wasm32` v128
//!   intrinsics (the four RGBA channels of a pixel accumulated together in an
//!   `i32x4` lane group).
//!
//! Both divide the per-pixel window sum by the same window count with the SAME
//! integer division, so the SIMD result is bit-identical to the scalar one — the
//! correctness gate in `src/host/native-guest/__tests__/blur-demo-guest.test.ts`
//! asserts exactly that. This is the reference for #57's "documented +simd128
//! build path + one baseline filter's SIMD version, verifiable byte-for-byte".
//!
//! The SIMD path is enabled per function via `#[target_feature(enable =
//! "simd128")]`, so the crate builds and vectorizes with NO global
//! `-C target-feature=+simd128` flag — the existing byte-reproducible fixtures
//! and the flagless CI `cargo build`/`clippy` are left untouched. wasm SIMD128
//! is emitted as opcodes, not imports, so the guest still passes the host-import
//! seal unchanged.
#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use core::arch::wasm32::*;

keel_native_guest::guest_main!(guest_main);

async fn guest_main() {
    // No lifecycle behaviour: this fixture exists for the exported blur hooks the
    // host test drives directly. keel_init still runs so the guest loads cleanly.
}

const BYTES_PER_PIXEL: usize = 4;

/// Allocate a zeroed byte buffer in the guest's linear memory and hand its
/// pointer to the host. Intentionally leaked: the host writes source pixels into
/// it and reads filtered pixels back across separate calls, and the guest is
/// torn down after the test, so the leak is bounded and deliberate.
#[no_mangle]
pub extern "C" fn blur_alloc(len: usize) -> *mut u8 {
    let buf = alloc::vec![0u8; len].into_boxed_slice();
    Box::leak(buf).as_mut_ptr()
}

/// Scalar reference box blur: RGBA8, straight-alpha, row-major (stride = `w*4`),
/// square window of radius `r` with edges clamped. Reads `w*h*4` bytes from
/// `src` and writes `w*h*4` bytes to `dst`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // wasm ABI export; host owns src/dst (blur_alloc)
pub extern "C" fn box_blur_scalar(src: *const u8, dst: *mut u8, w: u32, h: u32, r: u32) {
    let n = w as usize * h as usize * BYTES_PER_PIXEL;
    // SAFETY: the host allocated src/dst via blur_alloc(w*h*4) and passes matching dims.
    let src = unsafe { core::slice::from_raw_parts(src, n) };
    let dst = unsafe { core::slice::from_raw_parts_mut(dst, n) };
    blur_scalar(src, dst, w as usize, h as usize, r as usize);
}

/// SIMD box blur — byte-for-byte identical to [`box_blur_scalar`]. The per-pixel
/// window is accumulated across the four RGBA channels at once in an `i32x4`
/// (v128) lane group; the final divide-by-count is done per lane with the same
/// integer division the scalar path uses, so the rounding matches exactly.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // wasm ABI export; host owns src/dst (blur_alloc)
pub extern "C" fn box_blur_simd(src: *const u8, dst: *mut u8, w: u32, h: u32, r: u32) {
    let n = w as usize * h as usize * BYTES_PER_PIXEL;
    // SAFETY: as above; blur_simd reads exactly 4 in-bounds bytes per sample.
    let src = unsafe { core::slice::from_raw_parts(src, n) };
    let dst = unsafe { core::slice::from_raw_parts_mut(dst, n) };
    unsafe { blur_simd(src, dst, w as usize, h as usize, r as usize) };
}

/// Clamp a (possibly negative) coordinate into `0..hi` for edge handling.
#[inline]
fn clamp_coord(v: isize, hi: usize) -> usize {
    if v < 0 {
        0
    } else if v as usize >= hi {
        hi - 1
    } else {
        v as usize
    }
}

fn blur_scalar(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let ri = r as isize;
    let count = ((2 * r + 1) * (2 * r + 1)) as u32;
    for y in 0..h {
        for x in 0..w {
            let (mut sr, mut sg, mut sb, mut sa) = (0u32, 0u32, 0u32, 0u32);
            for dy in -ri..=ri {
                let yy = clamp_coord(y as isize + dy, h);
                for dx in -ri..=ri {
                    let xx = clamp_coord(x as isize + dx, w);
                    let i = (yy * w + xx) * BYTES_PER_PIXEL;
                    sr += src[i] as u32;
                    sg += src[i + 1] as u32;
                    sb += src[i + 2] as u32;
                    sa += src[i + 3] as u32;
                }
            }
            let o = (y * w + x) * BYTES_PER_PIXEL;
            dst[o] = (sr / count) as u8;
            dst[o + 1] = (sg / count) as u8;
            dst[o + 2] = (sb / count) as u8;
            dst[o + 3] = (sa / count) as u8;
        }
    }
}

#[target_feature(enable = "simd128")]
unsafe fn blur_simd(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let ri = r as isize;
    let count = ((2 * r + 1) * (2 * r + 1)) as u32;
    for y in 0..h {
        for x in 0..w {
            let mut acc = i32x4_splat(0);
            for dy in -ri..=ri {
                let yy = clamp_coord(y as isize + dy, h);
                for dx in -ri..=ri {
                    let xx = clamp_coord(x as isize + dx, w);
                    let i = (yy * w + xx) * BYTES_PER_PIXEL;
                    // Load the 4 channel bytes into the low 32 bits, widen u8 -> u32,
                    // accumulate all four channels in one add.
                    let px = v128_load32_zero(src.as_ptr().add(i) as *const u32);
                    let px16 = u16x8_extend_low_u8x16(px);
                    let px32 = u32x4_extend_low_u16x8(px16);
                    acc = i32x4_add(acc, px32);
                }
            }
            let o = (y * w + x) * BYTES_PER_PIXEL;
            dst[o] = (i32x4_extract_lane::<0>(acc) as u32 / count) as u8;
            dst[o + 1] = (i32x4_extract_lane::<1>(acc) as u32 / count) as u8;
            dst[o + 2] = (i32x4_extract_lane::<2>(acc) as u32 / count) as u8;
            dst[o + 3] = (i32x4_extract_lane::<3>(acc) as u32 / count) as u8;
        }
    }
}
