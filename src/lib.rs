use std::{
    error, fmt,
    io::{self, Read, Write},
};

pub trait Encode: Sized {
    fn encode<W: Write>(&self, writer: &mut W) -> io::Result<usize>;
}

pub trait Decode: Sized {
    fn decode<R: Read>(reader: &mut R) -> io::Result<Self>;
}

#[macro_export]
macro_rules! derive_enum {
    ($name:ident {
        $($variant:ident),+
    }) => {
        #[derive(Debug)]
        #[repr(u32)]
        pub enum $name {
            $($variant),+
        }

        impl crate::networking::messaging::Encode for $name {
            fn encode<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<usize> {
                let len = match self {
                    $(Self::$variant => writer.write(&(Self::$variant as u32).to_be_bytes())?),+
                };

                Ok(len)
            }
        }

        impl crate::networking::messaging::Decode for $name {
            fn decode<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                let mut variant_header = [0; size_of::<u32>()];
                reader.read_exact(&mut variant_header)?;

                let variant = u32::from_be_bytes(variant_header);
                match variant {
                    $(val if val == Self::$variant as u32 => Ok(Self::$variant),)+
                    _ => Err(std::io::Error::from(std::io::ErrorKind::NotFound))
                }
            }
        }
    };
}

#[macro_export]
macro_rules! derive_struct {
    ($name:ident {
        $($field:ident : $type:path),*
    }) => {
        #[derive(Debug)]
        pub struct $name {
            $($field : $type),*
        }

        impl $crate::networking::messaging::Encode for $name {
            fn encode<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<usize> {
                let mut len = 0;
                $(len += self.$field.encode(writer)?;)*

                Ok(len)
            }
        }

        impl $crate::networking::messaging::Decode for $name {
            fn decode<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                Ok(Self {
                    $($field : <$type>::decode(reader)?),*
                })
            }
        }
    }
}

macro_rules! impl_encode_number {
    ($($primitive:ty),+) => {
        $(
            impl self::Encode for $primitive {
                fn encode<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
                    writer.write(&self.to_be_bytes())
                }
            }
        )+
    };
}

impl_encode_number!(u8, u16, u32, u64, u128, usize);

impl<T: self::Encode> self::Encode for Vec<T> {
    fn encode<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
        let len_header = self.len().encode(writer)?;
        let len_body = self
            .iter()
            .try_fold(0, |acc, elem| elem.encode(writer).map(|len| acc + len))?;

        Ok(len_header + len_body)
    }
}

impl self::Encode for String {
    fn encode<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
        let len_header = self.len().encode(writer)?;
        let len_body = writer.write(&self.as_bytes())?;

        Ok(len_header + len_body)
    }
}

macro_rules! impl_decode_number {
    ($($primitive:ty),+) => {
        $(
            impl self::Decode for $primitive {
                fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
                    let mut buffer = [0; size_of::<$primitive>()];
                    reader.read_exact(&mut buffer)?;
                    Ok(<$primitive>::from_be_bytes(buffer))
                }
            }
        )+
    };
}

impl_decode_number!(u8, u16, u32, u64, u128, usize);

impl<T: self::Decode> self::Decode for Vec<T> {
    fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut buffer_header = [0; size_of::<usize>()];
        reader.read_exact(&mut buffer_header)?;

        let len_body = usize::from_be_bytes(buffer_header);
        let buffer_body = (0..len_body)
            .map(|_| T::decode(reader))
            .collect::<io::Result<Self>>()?;

        Ok(buffer_body)
    }
}

impl self::Decode for String {
    fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut buffer_header = [0; size_of::<usize>()];
        reader.read_exact(&mut buffer_header)?;

        let len_body = usize::from_be_bytes(buffer_header);
        let mut buffer_body = vec![0; len_body];
        reader.read_exact(buffer_body.as_mut_slice())?;

        Ok(String::from_utf8(buffer_body).unwrap())
    }
}

macro_rules! generate_number_tests {
    ($($primitive:ident),+) => {
        $(
            proptest! {
                #[test]
                fn $primitive(number: $primitive) {
                    let mut cursor = io::Cursor::new(vec![]);
                    number.encode(&mut cursor).unwrap();

                    cursor.set_position(0);
                    prop_assert_eq!(<$primitive>::decode(&mut cursor).unwrap(), number);
                }
            }
        )+
    };
}

macro_rules! generate_vector_tests {
    ($($primitive:ident),+) => {
        $(
            proptest! {
                #[test]
                #[allow(non_snake_case)]
                fn $primitive(v: Vec<$primitive>) {
                    let mut cursor = io::Cursor::new(vec![]);
                    v.encode(&mut cursor).unwrap();

                    cursor.set_position(0);
                    let decoded = Vec::<$primitive>::decode(&mut cursor).unwrap();
                    prop_assert_eq!(decoded, v);
                }
            }
        )+
    };
}

#[derive(Debug)]
pub enum Error {
    KindInvalid(u8),
    Decode(io::Error),
}

impl fmt::Display for self::Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KindInvalid(kind) => write!(f, "{kind} isn't a valid message kind"),
            Self::Decode(src) => write!(f, "failed to decode: {src}"),
        }
    }
}

impl error::Error for self::Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::KindInvalid(_kind) => None,
            Self::Decode(src) => Some(src),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use proptest::prelude::*;

    mod number {
        use super::*;
        generate_number_tests!(u8, u16, u32, u64, u128);
    }

    mod vector {
        use super::*;
        generate_vector_tests!(u8, u16, u32, u64, u128, String);
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Example {
        id: u128,
        name: String,
        address: String,
        age: u16,
        phone: String,
    }

    impl super::Encode for self::Example {
        fn encode<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
            let len_id = self.id.encode(writer)?;
            let len_name = self.name.encode(writer)?;
            let len_address = self.address.encode(writer)?;
            let len_age = self.age.encode(writer)?;
            let len_phone = self.phone.encode(writer)?;
            Ok(len_id + len_name + len_address + len_age + len_phone)
        }
    }

    impl super::Decode for self::Example {
        fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
            let id = u128::decode(reader)?;
            let name = String::decode(reader)?;
            let address = String::decode(reader)?;
            let age = u16::decode(reader)?;
            let phone = String::decode(reader)?;
            Ok(Self {
                id,
                name,
                address,
                age,
                phone,
            })
        }
    }

    proptest! {
        #[test]
        fn string(s: String) {
            let mut cursor = io::Cursor::new(Vec::with_capacity(s.len()));
            s.encode(&mut cursor).unwrap();

            cursor.set_position(0);
            let decoded = String::decode(&mut cursor).unwrap();
            prop_assert_eq!(decoded, s);
        }

        #[test]
        fn example_struct(id: u128, name: String, address: String, age: u16, phone: String) {
            let example = self::Example { id, name, address, age, phone };
            let mut cursor = io::Cursor::new(vec![]);
            example.encode(&mut cursor).unwrap();

            cursor.set_position(0);
            let decoded = self::Example::decode(&mut cursor).unwrap();
            prop_assert_eq!(decoded, example);

        }
    }
}
