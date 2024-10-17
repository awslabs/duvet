// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use globset as g;
use serde::de;
use std::{str::FromStr, sync::Arc};

#[derive(Clone, Debug)]
pub struct Glob {
    set: Arc<g::GlobSet>,
}

impl Glob {
    pub fn is_match<P: AsRef<std::path::Path>>(&self, path: &P) -> bool {
        self.set.is_match(path)
    }

    pub fn try_from_iter<T: IntoIterator<Item = I>, I: AsRef<str>>(
        iter: T,
    ) -> Result<Glob, g::Error> {
        let mut builder = g::GlobSetBuilder::new();
        for item in iter {
            builder.add(g::Glob::new(item.as_ref())?);
        }
        let set = builder.build()?;
        let set = Arc::new(set);
        Ok(Self { set })
    }
}

impl FromStr for Glob {
    type Err = g::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from_iter(core::iter::once(value))
    }
}

impl<'de> de::Deserialize<'de> for Glob {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_any(StringOrList)
    }
}

struct StringOrList;

impl<'de> de::Visitor<'de> for StringOrList {
    type Value = Glob;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string or list of strings")
    }

    fn visit_str<E>(self, value: &str) -> Result<Glob, E>
    where
        E: de::Error,
    {
        value.parse().map_err(serde::de::Error::custom)
    }

    fn visit_seq<S>(self, mut seq: S) -> Result<Glob, S::Error>
    where
        S: de::SeqAccess<'de>,
    {
        let mut builder = g::GlobSetBuilder::new();
        while let Some(value) = seq.next_element()? {
            let item = g::Glob::new(value).map_err(serde::de::Error::custom)?;
            builder.add(item);
        }
        let set = builder.build().map_err(serde::de::Error::custom)?;
        let set = Arc::new(set);
        Ok(Glob { set })
    }
}
