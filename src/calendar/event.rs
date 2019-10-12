use chrono::prelude::*;

use libical_sys::{
    icalcomponent_get_description, icalcomponent_get_dtend, icalcomponent_get_dtstart,
    icalcomponent_get_summary, icaltime_as_timet, icaltime_is_null_time,
};

#[derive(Debug)]
pub struct Event {
    pub summary: String,
    pub description: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Event {
    pub(crate) fn from_component(component: *mut libical_sys::icalcomponent) -> Self {
        let summary = {
            let x = unsafe { icalcomponent_get_summary(component) };
            if x == 0 as _ {
                "".to_string()
            } else {
                unsafe { std::ffi::CStr::from_ptr(x) }
                    .to_string_lossy()
                    .to_string()
            }
        };

        let description = {
            let x = unsafe { icalcomponent_get_description(component) };
            if x == 0 as _ {
                "".to_string()
            } else {
                unsafe { std::ffi::CStr::from_ptr(x) }
                    .to_string_lossy()
                    .to_string()
            }
        };
        let start = {
            let mut raw = unsafe { icalcomponent_get_dtstart(component) };
            if unsafe { icaltime_is_null_time(raw) } == 1 {
                Utc.timestamp(0, 0)
            } else {
                let timet = unsafe { icaltime_as_timet(raw) };
                Utc.timestamp(timet, 0)
            }
        };
        let end = {
            let mut raw = unsafe { icalcomponent_get_dtend(component) };
            if unsafe { icaltime_is_null_time(raw) } == 1 {
                Utc.timestamp(0, 0)
            } else {
                let timet = unsafe { icaltime_as_timet(raw) };
                Utc.timestamp(timet, 0)
            }
        };
        Self {
            summary,
            description,
            start,
            end,
        }
    }

    pub(crate) fn starting_at(mut self, t: libical_sys::time_t) -> Self {
        let duration = self.end - self.start;
        self.start = Utc.timestamp(t, 0);
        self.end = self.start + duration;
        self
    }
}
