use stellar_baselib::xdr::next::{ScSpecType, ScVal};

pub fn native_to_sc_val(input: Vec<&str>, ty: ScSpecType) -> Result<ScVal, &'static str> {
    match ty {
        ScSpecType::Vec => {
            let ve22: Vec<ScVal> = input
                .into_iter()
                .map(|s| ScVal::Symbol(s.try_into().unwrap()))
                .collect();
            Ok(ScVal::Vec(Some(ve22.try_into().unwrap())))
        }
        ScSpecType::Tuple => todo!(),
        ScSpecType::Val => todo!(),
        ScSpecType::Bool => todo!(),
        ScSpecType::Void => todo!(),
        ScSpecType::Error => todo!(),
        ScSpecType::U32 => todo!(),
        ScSpecType::I32 => todo!(),
        ScSpecType::U64 => todo!(),
        ScSpecType::I64 => todo!(),
        ScSpecType::Timepoint => todo!(),
        ScSpecType::Duration => todo!(),
        ScSpecType::U128 => todo!(),
        ScSpecType::I128 => todo!(),
        ScSpecType::U256 => todo!(),
        ScSpecType::I256 => todo!(),
        ScSpecType::Bytes => todo!(),
        ScSpecType::String => todo!(),
        ScSpecType::Symbol => todo!(),
        ScSpecType::Address => todo!(),
        ScSpecType::Option => todo!(),
        ScSpecType::Result => todo!(),
        ScSpecType::Map => todo!(),
        ScSpecType::BytesN => todo!(),
        ScSpecType::Udt => todo!(),
    }
}
