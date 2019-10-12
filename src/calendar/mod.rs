use libical_sys::{icalparser_parse_string, icalcomponent, icalcomponent_free,
icalcomponent_kind_ICAL_VEVENT_COMPONENT as ICAL_VEVENT_COMPONENT,
icalcomponent_kind_ICAL_ANY_COMPONENT as ICAL_ANY_COMPONENT,
icalproperty_kind_ICAL_DESCRIPTION_PROPERTY as ICAL_DESCRIPTION_PROPERTY,
icalproperty_kind_ICAL_DTSTART_PROPERTY as ICAL_DTSTART_PROPERTY,
icalproperty_kind_ICAL_DTEND_PROPERTY as ICAL_DTEND_PROPERTY,
icalproperty_kind_ICAL_RRULE_PROPERTY as ICAL_RRULE_PROPERTY,
};
use std::ffi::{CStr, CString};

trait Calendar {
    fn get_current_event(&self);
    fn get_next_event(&self);
}

#[derive(Debug)]
enum Error {
    Parser,
    FfiNul,
}

impl From<std::ffi::NulError> for Error {
    fn from(e: std::ffi::NulError) -> Error {
        Error::FfiNul
    }
}

type Result<T> = std::result::Result<T, Error>;

struct Ical {
    calendar: *mut icalcomponent,
}

impl Ical {
    fn new_from_str(data: impl AsRef<str>) -> Result<Ical> {
        let s : CString = CString::new(data.as_ref())?;
        let calendar = unsafe { icalparser_parse_string(s.as_ptr()) };

        if calendar == 0 as _ {
            return Err(Error::FfiNul);
        }

        Ok(Ical {
            calendar,
        })
    }

    fn print_events(&mut self) {
        let now = std::time::SystemTime::now();

        let mut it : libical_sys::icalcompiter = unsafe { libical_sys::icalcomponent_begin_component(self.calendar, ICAL_VEVENT_COMPONENT) };
        while unsafe { libical_sys::icalcompiter_deref(&mut it) } != 0 as _ {
            let item = unsafe { libical_sys::icalcompiter_deref(&mut it) };

            let desc = unsafe { libical_sys::icalcomponent_get_first_property(item, ICAL_DESCRIPTION_PROPERTY) };
            let dtstart = unsafe { libical_sys::icalcomponent_get_first_property(item, ICAL_DTSTART_PROPERTY) };
            let dtend = unsafe { libical_sys::icalcomponent_get_first_property(item, ICAL_DTEND_PROPERTY) };
            let rrule = unsafe { libical_sys::icalcomponent_get_first_property(item, ICAL_RRULE_PROPERTY) };

            if desc == 0 as _ || dtstart == 0 as _ || dtend == 0 as _ {
                unsafe { libical_sys::icalcompiter_next(&mut it) };
                continue
            }

            let desc = unsafe { CStr::from_ptr(libical_sys::icalproperty_get_description(desc) )};
            let start = unsafe { libical_sys::icalproperty_get_dtstart(dtstart) };
            let end = unsafe { libical_sys::icalproperty_get_dtend(dtend) };
            let ttstart = convert_timet(unsafe { libical_sys::icaltime_as_timet(start) });
            let ttend = convert_timet(unsafe { libical_sys::icaltime_as_timet(end) });

            if rrule == 0 as _ {
                if ttstart > now && ttstart < now + std::time::Duration::from_secs(3600 * 24 * 30) {
                    println!("[{:?}] {:?} {:?}", desc, ttstart, ttend);
                }
            } else {
                let recur = unsafe { libical_sys::icalproperty_get_rrule(rrule) };
                let ritr = unsafe { libical_sys::icalrecur_iterator_new(recur, start) };

                let mut next = unsafe { libical_sys::icalrecur_iterator_next(ritr) };
                while unsafe { libical_sys::icaltime_is_null_time(next) } == 0 {
                    let time = unsafe { libical_sys::icaltime_as_timet(next) };
                    let time = convert_timet(time);

                    if time > now + std::time::Duration::from_secs(3600 * 24 * 30) {
                        break
                    }

                    if time > now {
                        println!("{:?} – {:?}", desc, time);
                    }
                    next = unsafe { libical_sys::icalrecur_iterator_next(ritr) };
                }
                unsafe { libical_sys::icalrecur_iterator_free(ritr); };
            }

            unsafe { libical_sys::icalcompiter_next(&mut it) };
        }

    }
}

impl<'a> IntoIterator for &'a Ical {
    type Item = std::time::SystemTime;
    type IntoIter = IcalIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        IcalIterator::new(&self)
    }
}

enum IcalIterVeventState {
    NoRecur,
    Recur(*mut libical_sys::icalrecur_iterator),
    Done,
}

impl Drop for IcalIterVeventState {
    fn drop(&mut self) {
        if let IcalIterVeventState::Recur(ritr) = self {
            unsafe { libical_sys::icalrecur_iterator_free(*ritr) };
        }
    }
}

struct IcalIterVevent {
    component: *mut libical_sys::icalcomponent,
    ritr: IcalIterVeventState,
}

impl IcalIterVevent {
    fn new(component: *mut libical_sys::icalcomponent) -> Self {
        let rrule = unsafe { libical_sys::icalcomponent_get_first_property(component, ICAL_RRULE_PROPERTY) };
        let dtstart = unsafe { libical_sys::icalcomponent_get_first_property(component, ICAL_DTSTART_PROPERTY) };
        let start = unsafe { libical_sys::icalproperty_get_dtstart(dtstart) };

        let ritr = if rrule != 0 as _ {
            let recur = unsafe { libical_sys::icalproperty_get_rrule(rrule) };
            let ritr = unsafe { libical_sys::icalrecur_iterator_new(recur, start) };
            IcalIterVeventState::Recur(ritr)
        } else { IcalIterVeventState::NoRecur };
        Self {
            component,
            ritr,
        }
    }
}

impl Iterator for IcalIterVevent {
    type Item = std::time::SystemTime;

    fn next(&mut self) -> Option<Self::Item> {
        let dtstart = unsafe { libical_sys::icalcomponent_get_first_property(self.component, ICAL_DTSTART_PROPERTY) };
        let start = unsafe { libical_sys::icalproperty_get_dtstart(dtstart) };

        match self.ritr {
            IcalIterVeventState::Done => None,
            IcalIterVeventState::NoRecur => {
                // yield self and be done
                self.ritr = IcalIterVeventState::Done;
                let time = unsafe { libical_sys::icaltime_as_timet(start) };
                Some(convert_timet(time))
            },
            IcalIterVeventState::Recur(r) => {
                let item = unsafe { libical_sys::icalrecur_iterator_next(r) };
                if unsafe { libical_sys::icaltime_is_null_time(item) } != 0 {
                    self.ritr = IcalIterVeventState::Done;
                    None
                } else {
                    let time = unsafe { libical_sys::icaltime_as_timet(item) };
                    let time = convert_timet(time);
                    Some(time)
                }
            }
        }
    }
}

enum IterState {
    Recurse(IcalIterVevent),
    Done,
}

impl IterState {
    #[inline]
    fn is_init(&self) -> bool {
        match self {
            Init => true,
            _ => false
        }
    }
}

struct IcalIterator<'a> {
    ical: &'a Ical,
    vevent_iterator: libical_sys::icalcompiter,
    state: Option<IterState>
}

impl<'a> IcalIterator<'a> {
    pub fn new(ical: &'a Ical) -> Self {
        let vevent_iterator: libical_sys::icalcompiter = unsafe { libical_sys::icalcomponent_begin_component(ical.calendar, ICAL_VEVENT_COMPONENT) };

        Self {
            ical,
            vevent_iterator,
            state: None,
        }
    }

    fn next_vevent(&mut self) -> Option<IcalIterVevent> {
        let item = unsafe { libical_sys::icalcompiter_next(&mut self.vevent_iterator) };
        let next = unsafe { libical_sys::icalcompiter_deref(&mut self.vevent_iterator) };
        if next == 0 as _ {
            None
        } else {
            Some(IcalIterVevent::new(item))
        }
    }
}

impl<'a> Iterator for IcalIterator<'a> {
    type Item = std::time::SystemTime;

    fn next(&mut self) -> Option<Self::Item> {
        if self.state.is_none() {
            self.state = match self.next_vevent() {
                Some(i) => Some(IterState::Recurse(i)),
                None => Some(IterState::Done)
            };
        }

        match &mut self.state {
            None | Some(IterState::Done) => None,
            Some(IterState::Recurse(i)) => {
                let value = i.next();
                if value == None {
                    match self.next_vevent() {
                        None => {
                            self.state = Some(IterState::Done);
                            None
                        },
                        Some(v) => {
                            self.state = Some(IterState::Recurse(v));
                            self.next()
                        }
                    }
                } else {
                    value
                }
            }
        }
    }
}

fn convert_timet(time: libical_sys::time_t) -> std::time::SystemTime {
    let elapsed = std::time::Duration::from_secs(time as _);
    std::time::SystemTime::UNIX_EPOCH.checked_add(elapsed).unwrap()
}

impl Drop for Ical {
    fn drop(&mut self) {
        if self.calendar != 0 as _ {
            unsafe {
                icalcomponent_free(self.calendar);
                self.calendar = 0 as _;
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::IntoFuture;
    use futures::Future;

    fn run_one<F>(f: F) -> std::result::Result<F::Item, F::Error>
    where
        F: IntoFuture,
        F::Future: Send + 'static,
        F::Item: Send + 'static,
        F::Error: Send + 'static,
    {
        let mut runtime = tokio::runtime::Runtime::new().expect("Unable to create a runtime");
        runtime.block_on(f.into_future())
    }

    #[test]
    fn test_ical_decode() {
        let client = reqwest::r#async::ClientBuilder::new().build().unwrap();
        let fut = client.get("https://davical.darmstadt.ccc.de/public.php/cda/public/").send()
            .and_then(|mut r| r.text());
        let text = run_one(fut).unwrap();
        let mut ical = Ical::new_from_str(text).unwrap();
        ical.print_events();
    }

    #[test]
    fn test_ical_iter() {
        let client = reqwest::r#async::ClientBuilder::new().build().unwrap();
        let fut = client.get("https://davical.darmstadt.ccc.de/public.php/cda/public/").send()
            .and_then(|mut r| r.text());
        let text = run_one(fut).unwrap();
        let ical = Ical::new_from_str(text).unwrap();
        assert!((&ical).into_iter().count() > 0);
        for (i, ts) in (&ical).into_iter().enumerate() {
            println!("{} {:?}", i, ts);
        }
    }
}
