// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use console::style;
use core::fmt;
use std::time::Instant;

#[macro_export]
macro_rules! progress {
    ($progress:ident, $fmt:literal $($tt:tt)*) => {
        $progress.finish(format_args!($fmt $($tt)*));
    };
    ($fmt:literal $($tt:tt)*) => {
        $crate::progress::Progress::new(format_args!($fmt $($tt)*))
    };
}

pub struct Progress {
    start_time: Instant,
}

impl Progress {
    pub fn new<T: fmt::Display>(v: T) -> Self {
        let start_time = Instant::now();
        let v = v.to_string();
        if let Some((status, info)) = v.split_once(' ') {
            let status = style(status).cyan().bold();
            eprintln!("{status:>12} {info}")
        } else {
            eprintln!("{v}");
        }
        Self { start_time }
    }

    pub fn finish<T: fmt::Display>(self, v: T) {
        let total = self.total_time();
        let total = style(&total).dim();

        let v = v.to_string();
        if let Some((status, info)) = v.split_once(' ') {
            let status = style(status).green().bold();
            eprintln!("{status:>12} {info} {total}")
        } else {
            eprintln!("{v} {total}");
        }
    }

    fn total_time(&self) -> String {
        let total = self.start_time.elapsed();

        if total.as_secs() > 0 {
            format!("{:.2}s", total.as_secs_f32())
        } else if total.as_millis() > 0 {
            format!("{}ms", total.as_millis())
        } else if total.as_micros() > 0 {
            format!("{}µs", total.as_micros())
        } else {
            format!("{total:?}")
        }
    }
}
