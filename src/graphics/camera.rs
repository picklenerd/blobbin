use winit::event::{ElementState, KeyboardInput, VirtualKeyCode, WindowEvent};

pub struct Camera {
    pub eye: cgmath::Point3<f32>,
    pub up: cgmath::Vector3<f32>,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn build_view_projection_matrix(&self) -> cgmath::Matrix4<f32> {
        let target = (self.eye.x, self.eye.y, self.eye.z - 1.0).into();
        let view = cgmath::Matrix4::look_at(self.eye, target, self.up);
        let proj = cgmath::perspective(cgmath::Deg(self.fovy), self.aspect, self.znear, self.zfar);
        return OPENGL_TO_WGPU_MATRIX * proj * view;
    }
}

#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: cgmath::Matrix4<f32> = cgmath::Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);

pub struct CameraController {
    speed: f32,
    x_axis: f32,
    y_axis: f32,
    z_axis: f32,
    speed_multiplier: f32,
}

impl CameraController {
    pub fn new(speed: f32) -> Self {
        Self {
            speed,
            x_axis: 0.0,
            y_axis: 0.0,
            z_axis: 0.0,
            speed_multiplier: 1.0,
        }
    }

    pub fn process_events(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        state,
                        virtual_keycode: Some(keycode),
                        ..
                    },
                ..
            } => {
                let is_pressed = *state == ElementState::Pressed;
                let axis_value = if is_pressed { 1.0 } else { 0.0 };
                match keycode {
                    VirtualKeyCode::A | VirtualKeyCode::Left => {
                        self.x_axis = -axis_value;
                        true
                    }
                    VirtualKeyCode::D | VirtualKeyCode::Right => {
                        self.x_axis = axis_value;
                        true
                    }
                    VirtualKeyCode::W | VirtualKeyCode::Up => {
                        self.y_axis = axis_value;
                        true
                    }
                    VirtualKeyCode::S | VirtualKeyCode::Down => {
                        self.y_axis = -axis_value;
                        true
                    }
                    VirtualKeyCode::R | VirtualKeyCode::E => {
                        self.z_axis = -axis_value;
                        true
                    }
                    VirtualKeyCode::F | VirtualKeyCode::Q => {
                        self.z_axis = axis_value;
                        true
                    }
                    VirtualKeyCode::LShift | VirtualKeyCode::RShift => {
                        self.speed_multiplier = if is_pressed { 2.0 } else { 1.0 };
                        true
                    }
                    VirtualKeyCode::LAlt | VirtualKeyCode::RAlt => {
                        self.speed_multiplier = if is_pressed { 0.25 } else { 1.0 };
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn update_camera(&self, camera: &mut Camera) {
        camera.eye.x += self.x_axis * self.speed * self.speed_multiplier;
        camera.eye.y += self.y_axis * self.speed * self.speed_multiplier;
        camera.eye.z += self.z_axis * self.speed * self.speed_multiplier;
    }
}
