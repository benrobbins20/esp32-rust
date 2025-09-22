use accel_stepper::Device;
use esp_idf_hal::gpio::{PinDriver, OutputPin, Output};

pub struct Stepper <N: OutputPin, E: OutputPin, S: OutputPin, W: OutputPin> {
    p1: PinDriver<'static, N, Output>,
    p2: PinDriver<'static, E, Output>,
    p3: PinDriver<'static, S, Output>,
    p4: PinDriver<'static, W, Output>,
}

// impl device trait for accel_stepper
impl<N: OutputPin, E: OutputPin, S: OutputPin, W: OutputPin> Device for Stepper<N, E, S, W> {
    type Error = ();

    fn step(&mut self, ctx: &accel_stepper::StepContext) -> Result<(), Self::Error> {
        self.step(ctx.position);
        Ok(())
    }
}

// generic implementation for stepper
impl <N: OutputPin, E: OutputPin, S: OutputPin, W: OutputPin> Stepper <N, E, S, W> {
    pub fn new(p1: N, p2: E, p3: S, p4: W) -> Self {
        Self {
            p1: PinDriver::output(p1).unwrap(),
            p2: PinDriver::output(p2).unwrap(),
            p3: PinDriver::output(p3).unwrap(),
            p4: PinDriver::output(p4).unwrap(),
        } 
    }

    pub fn step(&mut self, step: i64) {

        // simple stepping method
        if step.rem_euclid(4) == 0 {
            self.p1.set_high().unwrap();
            self.p2.set_low().unwrap();
            self.p3.set_low().unwrap();
            self.p4.set_low().unwrap();
        }
        if step.rem_euclid(4) == 1 {
            self.p1.set_low().unwrap();
            self.p2.set_high().unwrap();
            self.p3.set_low().unwrap();
            self.p4.set_low().unwrap();
        }
        if step.rem_euclid(4) == 2 {
            self.p1.set_low().unwrap();
            self.p2.set_low().unwrap();
            self.p3.set_high().unwrap();
            self.p4.set_low().unwrap();
        }
        if step.rem_euclid(4) == 3 {
            self.p1.set_low().unwrap();
            self.p2.set_low().unwrap();
            self.p3.set_low().unwrap();
            self.p4.set_high().unwrap();
        }
    }
    pub fn stop(&mut self) {
        self.p1.set_low().unwrap();
        self.p2.set_low().unwrap();
        self.p3.set_low().unwrap();
        self.p4.set_low().unwrap();
    } 
}