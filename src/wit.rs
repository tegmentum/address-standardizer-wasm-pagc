//! WASM `Guest` implementation: maps the `address-standardizer` WIT
//! interface to `ops`.

use crate::bindings::exports::tegmentum::address_standardizer_pagc::address_standardizer::{
    Guest, StandardizedAddress, StandardizerError,
};
use crate::ops;

struct Component;

fn into_wit(a: ops::StandardizedAddress) -> StandardizedAddress {
    StandardizedAddress {
        building: a.building,
        house_num: a.house_num,
        predir: a.predir,
        qual: a.qual,
        pretype: a.pretype,
        name: a.name,
        suftype: a.suftype,
        sufdir: a.sufdir,
        ruralroute: a.ruralroute,
        extra: a.extra,
        city: a.city,
        state: a.state,
        country: a.country,
        postcode: a.postcode,
        box_: a.r#box,
        unit: a.unit,
    }
}

fn from_wit(a: StandardizedAddress) -> ops::StandardizedAddress {
    ops::StandardizedAddress {
        building: a.building,
        house_num: a.house_num,
        predir: a.predir,
        qual: a.qual,
        pretype: a.pretype,
        name: a.name,
        suftype: a.suftype,
        sufdir: a.sufdir,
        ruralroute: a.ruralroute,
        extra: a.extra,
        city: a.city,
        state: a.state,
        country: a.country,
        postcode: a.postcode,
        r#box: a.box_,
        unit: a.unit,
    }
}

impl Guest for Component {
    fn standardize_address(addr: String) -> Result<StandardizedAddress, StandardizerError> {
        ops::standardize(&addr)
            .map(into_wit)
            .map_err(|message| StandardizerError { message })
    }

    fn parse_address(addr: String) -> Result<StandardizedAddress, StandardizerError> {
        ops::parse(&addr)
            .map(into_wit)
            .map_err(|message| StandardizerError { message })
    }

    fn as_text(addr: StandardizedAddress) -> String {
        ops::as_text(&from_wit(addr))
    }
}

crate::bindings::export!(Component with_types_in crate::bindings);
