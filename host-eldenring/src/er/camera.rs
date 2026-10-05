//! Elden Ring's camera at V1's eyes, looking where V1 looks (er-mario's lakitu.rs approach: the
//! render camera's matrix is rewritten at every step from the game's camera update to drawing,
//! because the game copies its own back in between and sets up culling and shadows from it).

use std::sync::Mutex;

use eldenring::cs::CSCamera;
use fromsoftware_shared::FromStatic;
use glam::Vec3;

/// right, up, forward, position (12 floats), the vertical fov in radians, and when it was set.
static LAST: Mutex<Option<([f32; 12], f32, std::time::Instant)>> = Mutex::new(None);

/// Sets the camera for this frame (Havok space).
/// `up` carries ULTRAKILL's camera roll (its tilt when strafing and sliding, and screen shake).
pub fn set(eye: Vec3, fwd: Vec3, up: Vec3, fov_deg: f32) {
    let Ok(camera) = (unsafe { CSCamera::instance_mut() }) else { return };
    let m = &camera.pers_cam_1.matrix;
    // keep the game's handedness: build right from up x forward and flip it if the game's is the
    // other way round
    let (gr, gu, gf) = (Vec3::new(m.0.0, m.0.1, m.0.2), Vec3::new(m.1.0, m.1.1, m.1.2), Vec3::new(m.2.0, m.2.1, m.2.2));
    let handed = gr.dot(gu.cross(gf)).signum();
    let fwd = fwd.normalize_or(Vec3::Z);
    // V1's up (with its roll) when it's usable, else the world's
    let up_in = if up.is_finite() && up.length_squared() > 0.5 && up.dot(fwd).abs() < 0.9 { up.normalize() } else { Vec3::Y };
    let mut right = up_in.cross(fwd).normalize_or(Vec3::X);
    let up = fwd.cross(right).normalize_or(Vec3::Y);
    if right.dot(up.cross(fwd)).signum() != handed {
        right = -right;
    }
    let up = if up.dot(up_in) < 0.0 { -up } else { up };
    let v = [right.x, right.y, right.z, up.x, up.y, up.z, fwd.x, fwd.y, fwd.z, eye.x, eye.y, eye.z];
    let fov = fov_deg.clamp(20.0, 160.0).to_radians();
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((v, fov, std::time::Instant::now()));
    reapply();
}

/// Stops overriding: the game's camera shows again.
pub fn release() {
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Writes the last camera into every camera of the set (run in many task groups).
pub fn reapply() {
    let Some((v, fov, at)) = *LAST.lock().unwrap_or_else(|e| e.into_inner()) else { return };
    if at.elapsed().as_secs_f32() > 0.1 {
        return; // V1's frames stopped (loading): the game's camera takes over
    }
    let Ok(camera) = (unsafe { CSCamera::instance_mut() }) else { return };
    for cam in [&mut camera.pers_cam_1, &mut camera.pers_cam_2, &mut camera.pers_cam_3, &mut camera.pers_cam_4] {
        let mm = &mut cam.matrix;
        (mm.0.0, mm.0.1, mm.0.2, mm.1.0, mm.1.1, mm.1.2) = (v[0], v[1], v[2], v[3], v[4], v[5]);
        (mm.2.0, mm.2.1, mm.2.2, mm.3.0, mm.3.1, mm.3.2) = (v[6], v[7], v[8], v[9], v[10], v[11]);
        cam.fov = fov;
    }
}
