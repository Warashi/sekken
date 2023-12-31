// @generated by the capnpc-rust plugin to the Cap'n Proto schema compiler.
// DO NOT EDIT.
// source: compact.capnp

pub mod compact_model {
    #[derive(Copy, Clone)]
    pub struct Owned(());
    impl ::capnp::introspect::Introspect for Owned {
        fn introspect() -> ::capnp::introspect::Type {
            ::capnp::introspect::TypeVariant::Struct(::capnp::introspect::RawBrandedStructSchema {
                generic: &_private::RAW_SCHEMA,
                field_types: _private::get_field_types,
                annotation_types: _private::get_annotation_types,
            })
            .into()
        }
    }
    impl ::capnp::traits::Owned for Owned {
        type Reader<'a> = Reader<'a>;
        type Builder<'a> = Builder<'a>;
    }
    impl ::capnp::traits::OwnedStruct for Owned {
        type Reader<'a> = Reader<'a>;
        type Builder<'a> = Builder<'a>;
    }
    impl ::capnp::traits::Pipelined for Owned {
        type Pipeline = Pipeline;
    }

    pub struct Reader<'a> {
        reader: ::capnp::private::layout::StructReader<'a>,
    }
    impl<'a> ::core::marker::Copy for Reader<'a> {}
    impl<'a> ::core::clone::Clone for Reader<'a> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<'a> ::capnp::traits::HasTypeId for Reader<'a> {
        const TYPE_ID: u64 = _private::TYPE_ID;
    }
    impl<'a> ::core::convert::From<::capnp::private::layout::StructReader<'a>> for Reader<'a> {
        fn from(reader: ::capnp::private::layout::StructReader<'a>) -> Self {
            Self { reader }
        }
    }

    impl<'a> ::core::convert::From<Reader<'a>> for ::capnp::dynamic_value::Reader<'a> {
        fn from(reader: Reader<'a>) -> Self {
            Self::Struct(::capnp::dynamic_struct::Reader::new(
                reader.reader,
                ::capnp::schema::StructSchema::new(::capnp::introspect::RawBrandedStructSchema {
                    generic: &_private::RAW_SCHEMA,
                    field_types: _private::get_field_types,
                    annotation_types: _private::get_annotation_types,
                }),
            ))
        }
    }

    impl<'a> ::core::fmt::Debug for Reader<'a> {
        fn fmt(
            &self,
            f: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::result::Result<(), ::core::fmt::Error> {
            core::fmt::Debug::fmt(
                &::core::convert::Into::<::capnp::dynamic_value::Reader<'_>>::into(*self),
                f,
            )
        }
    }

    impl<'a> ::capnp::traits::FromPointerReader<'a> for Reader<'a> {
        fn get_from_pointer(
            reader: &::capnp::private::layout::PointerReader<'a>,
            default: ::core::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Self> {
            ::core::result::Result::Ok(reader.get_struct(default)?.into())
        }
    }

    impl<'a> ::capnp::traits::IntoInternalStructReader<'a> for Reader<'a> {
        fn into_internal_struct_reader(self) -> ::capnp::private::layout::StructReader<'a> {
            self.reader
        }
    }

    impl<'a> ::capnp::traits::Imbue<'a> for Reader<'a> {
        fn imbue(&mut self, cap_table: &'a ::capnp::private::layout::CapTable) {
            self.reader
                .imbue(::capnp::private::layout::CapTableReader::Plain(cap_table))
        }
    }

    impl<'a> Reader<'a> {
        pub fn reborrow(&self) -> Reader<'_> {
            Self { ..*self }
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.reader.total_size()
        }
        #[inline]
        pub fn get_entries(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Reader<'a, crate::compact_capnp::compact_model::entry::Owned>,
        > {
            ::capnp::traits::FromPointerReader::get_from_pointer(
                &self.reader.get_pointer_field(0),
                ::core::option::Option::None,
            )
        }
        #[inline]
        pub fn has_entries(&self) -> bool {
            !self.reader.get_pointer_field(0).is_null()
        }
    }

    pub struct Builder<'a> {
        builder: ::capnp::private::layout::StructBuilder<'a>,
    }
    impl<'a> ::capnp::traits::HasStructSize for Builder<'a> {
        const STRUCT_SIZE: ::capnp::private::layout::StructSize =
            ::capnp::private::layout::StructSize {
                data: 0,
                pointers: 1,
            };
    }
    impl<'a> ::capnp::traits::HasTypeId for Builder<'a> {
        const TYPE_ID: u64 = _private::TYPE_ID;
    }
    impl<'a> ::core::convert::From<::capnp::private::layout::StructBuilder<'a>> for Builder<'a> {
        fn from(builder: ::capnp::private::layout::StructBuilder<'a>) -> Self {
            Self { builder }
        }
    }

    impl<'a> ::core::convert::From<Builder<'a>> for ::capnp::dynamic_value::Builder<'a> {
        fn from(builder: Builder<'a>) -> Self {
            Self::Struct(::capnp::dynamic_struct::Builder::new(
                builder.builder,
                ::capnp::schema::StructSchema::new(::capnp::introspect::RawBrandedStructSchema {
                    generic: &_private::RAW_SCHEMA,
                    field_types: _private::get_field_types,
                    annotation_types: _private::get_annotation_types,
                }),
            ))
        }
    }

    impl<'a> ::capnp::traits::ImbueMut<'a> for Builder<'a> {
        fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {
            self.builder
                .imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))
        }
    }

    impl<'a> ::capnp::traits::FromPointerBuilder<'a> for Builder<'a> {
        fn init_pointer(builder: ::capnp::private::layout::PointerBuilder<'a>, _size: u32) -> Self {
            builder
                .init_struct(<Self as ::capnp::traits::HasStructSize>::STRUCT_SIZE)
                .into()
        }
        fn get_from_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            default: ::core::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Self> {
            ::core::result::Result::Ok(
                builder
                    .get_struct(
                        <Self as ::capnp::traits::HasStructSize>::STRUCT_SIZE,
                        default,
                    )?
                    .into(),
            )
        }
    }

    impl<'a> ::capnp::traits::SetPointerBuilder for Reader<'a> {
        fn set_pointer_builder(
            mut pointer: ::capnp::private::layout::PointerBuilder<'_>,
            value: Self,
            canonicalize: bool,
        ) -> ::capnp::Result<()> {
            pointer.set_struct(&value.reader, canonicalize)
        }
    }

    impl<'a> Builder<'a> {
        pub fn into_reader(self) -> Reader<'a> {
            self.builder.into_reader().into()
        }
        pub fn reborrow(&mut self) -> Builder<'_> {
            Builder {
                builder: self.builder.reborrow(),
            }
        }
        pub fn reborrow_as_reader(&self) -> Reader<'_> {
            self.builder.as_reader().into()
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.builder.as_reader().total_size()
        }
        #[inline]
        pub fn get_entries(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Builder<'a, crate::compact_capnp::compact_model::entry::Owned>,
        > {
            ::capnp::traits::FromPointerBuilder::get_from_pointer(
                self.builder.get_pointer_field(0),
                ::core::option::Option::None,
            )
        }
        #[inline]
        pub fn set_entries(
            &mut self,
            value: ::capnp::struct_list::Reader<
                'a,
                crate::compact_capnp::compact_model::entry::Owned,
            >,
        ) -> ::capnp::Result<()> {
            ::capnp::traits::SetPointerBuilder::set_pointer_builder(
                self.builder.reborrow().get_pointer_field(0),
                value,
                false,
            )
        }
        #[inline]
        pub fn init_entries(
            self,
            size: u32,
        ) -> ::capnp::struct_list::Builder<'a, crate::compact_capnp::compact_model::entry::Owned>
        {
            ::capnp::traits::FromPointerBuilder::init_pointer(
                self.builder.get_pointer_field(0),
                size,
            )
        }
        #[inline]
        pub fn has_entries(&self) -> bool {
            !self.builder.is_pointer_field_null(0)
        }
    }

    pub struct Pipeline {
        _typeless: ::capnp::any_pointer::Pipeline,
    }
    impl ::capnp::capability::FromTypelessPipeline for Pipeline {
        fn new(typeless: ::capnp::any_pointer::Pipeline) -> Self {
            Self {
                _typeless: typeless,
            }
        }
    }
    impl Pipeline {}
    mod _private {
        pub static ENCODED_NODE: [::capnp::Word; 40] = [
            ::capnp::word(0, 0, 0, 0, 5, 0, 6, 0),
            ::capnp::word(108, 81, 44, 142, 168, 245, 252, 135),
            ::capnp::word(14, 0, 0, 0, 1, 0, 0, 0),
            ::capnp::word(210, 172, 237, 98, 200, 49, 55, 229),
            ::capnp::word(1, 0, 7, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(21, 0, 0, 0, 218, 0, 0, 0),
            ::capnp::word(33, 0, 0, 0, 23, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(41, 0, 0, 0, 63, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(99, 111, 109, 112, 97, 99, 116, 46),
            ::capnp::word(99, 97, 112, 110, 112, 58, 67, 111),
            ::capnp::word(109, 112, 97, 99, 116, 77, 111, 100),
            ::capnp::word(101, 108, 0, 0, 0, 0, 0, 0),
            ::capnp::word(4, 0, 0, 0, 1, 0, 1, 0),
            ::capnp::word(235, 176, 198, 13, 10, 88, 114, 232),
            ::capnp::word(1, 0, 0, 0, 50, 0, 0, 0),
            ::capnp::word(69, 110, 116, 114, 121, 0, 0, 0),
            ::capnp::word(4, 0, 0, 0, 3, 0, 4, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 1, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(13, 0, 0, 0, 66, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(8, 0, 0, 0, 3, 0, 1, 0),
            ::capnp::word(36, 0, 0, 0, 2, 0, 1, 0),
            ::capnp::word(101, 110, 116, 114, 105, 101, 115, 0),
            ::capnp::word(14, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 3, 0, 1, 0),
            ::capnp::word(16, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(235, 176, 198, 13, 10, 88, 114, 232),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(14, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
        ];
        pub fn get_field_types(index: u16) -> ::capnp::introspect::Type {
            match index {
        0 => <::capnp::struct_list::Owned<crate::compact_capnp::compact_model::entry::Owned> as ::capnp::introspect::Introspect>::introspect(),
        _ => panic!("invalid field index {}", index),
      }
        }
        pub fn get_annotation_types(
            child_index: Option<u16>,
            index: u32,
        ) -> ::capnp::introspect::Type {
            panic!("invalid annotation indices ({:?}, {}) ", child_index, index)
        }
        pub static RAW_SCHEMA: ::capnp::introspect::RawStructSchema =
            ::capnp::introspect::RawStructSchema {
                encoded_node: &ENCODED_NODE,
                nonunion_members: NONUNION_MEMBERS,
                members_by_discriminant: MEMBERS_BY_DISCRIMINANT,
            };
        pub static NONUNION_MEMBERS: &[u16] = &[0];
        pub static MEMBERS_BY_DISCRIMINANT: &[u16] = &[];
        pub const TYPE_ID: u64 = 0x87fc_f5a8_8e2c_516c;
    }

    pub mod entry {
        #[derive(Copy, Clone)]
        pub struct Owned(());
        impl ::capnp::introspect::Introspect for Owned {
            fn introspect() -> ::capnp::introspect::Type {
                ::capnp::introspect::TypeVariant::Struct(
                    ::capnp::introspect::RawBrandedStructSchema {
                        generic: &_private::RAW_SCHEMA,
                        field_types: _private::get_field_types,
                        annotation_types: _private::get_annotation_types,
                    },
                )
                .into()
            }
        }
        impl ::capnp::traits::Owned for Owned {
            type Reader<'a> = Reader<'a>;
            type Builder<'a> = Builder<'a>;
        }
        impl ::capnp::traits::OwnedStruct for Owned {
            type Reader<'a> = Reader<'a>;
            type Builder<'a> = Builder<'a>;
        }
        impl ::capnp::traits::Pipelined for Owned {
            type Pipeline = Pipeline;
        }

        pub struct Reader<'a> {
            reader: ::capnp::private::layout::StructReader<'a>,
        }
        impl<'a> ::core::marker::Copy for Reader<'a> {}
        impl<'a> ::core::clone::Clone for Reader<'a> {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl<'a> ::capnp::traits::HasTypeId for Reader<'a> {
            const TYPE_ID: u64 = _private::TYPE_ID;
        }
        impl<'a> ::core::convert::From<::capnp::private::layout::StructReader<'a>> for Reader<'a> {
            fn from(reader: ::capnp::private::layout::StructReader<'a>) -> Self {
                Self { reader }
            }
        }

        impl<'a> ::core::convert::From<Reader<'a>> for ::capnp::dynamic_value::Reader<'a> {
            fn from(reader: Reader<'a>) -> Self {
                Self::Struct(::capnp::dynamic_struct::Reader::new(
                    reader.reader,
                    ::capnp::schema::StructSchema::new(
                        ::capnp::introspect::RawBrandedStructSchema {
                            generic: &_private::RAW_SCHEMA,
                            field_types: _private::get_field_types,
                            annotation_types: _private::get_annotation_types,
                        },
                    ),
                ))
            }
        }

        impl<'a> ::core::fmt::Debug for Reader<'a> {
            fn fmt(
                &self,
                f: &mut ::core::fmt::Formatter<'_>,
            ) -> ::core::result::Result<(), ::core::fmt::Error> {
                core::fmt::Debug::fmt(
                    &::core::convert::Into::<::capnp::dynamic_value::Reader<'_>>::into(*self),
                    f,
                )
            }
        }

        impl<'a> ::capnp::traits::FromPointerReader<'a> for Reader<'a> {
            fn get_from_pointer(
                reader: &::capnp::private::layout::PointerReader<'a>,
                default: ::core::option::Option<&'a [::capnp::Word]>,
            ) -> ::capnp::Result<Self> {
                ::core::result::Result::Ok(reader.get_struct(default)?.into())
            }
        }

        impl<'a> ::capnp::traits::IntoInternalStructReader<'a> for Reader<'a> {
            fn into_internal_struct_reader(self) -> ::capnp::private::layout::StructReader<'a> {
                self.reader
            }
        }

        impl<'a> ::capnp::traits::Imbue<'a> for Reader<'a> {
            fn imbue(&mut self, cap_table: &'a ::capnp::private::layout::CapTable) {
                self.reader
                    .imbue(::capnp::private::layout::CapTableReader::Plain(cap_table))
            }
        }

        impl<'a> Reader<'a> {
            pub fn reborrow(&self) -> Reader<'_> {
                Self { ..*self }
            }

            pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
                self.reader.total_size()
            }
            #[inline]
            pub fn get_key(self) -> u64 {
                self.reader.get_data_field::<u64>(0)
            }
            #[inline]
            pub fn get_value(self) -> u8 {
                self.reader.get_data_field::<u8>(8)
            }
        }

        pub struct Builder<'a> {
            builder: ::capnp::private::layout::StructBuilder<'a>,
        }
        impl<'a> ::capnp::traits::HasStructSize for Builder<'a> {
            const STRUCT_SIZE: ::capnp::private::layout::StructSize =
                ::capnp::private::layout::StructSize {
                    data: 2,
                    pointers: 0,
                };
        }
        impl<'a> ::capnp::traits::HasTypeId for Builder<'a> {
            const TYPE_ID: u64 = _private::TYPE_ID;
        }
        impl<'a> ::core::convert::From<::capnp::private::layout::StructBuilder<'a>> for Builder<'a> {
            fn from(builder: ::capnp::private::layout::StructBuilder<'a>) -> Self {
                Self { builder }
            }
        }

        impl<'a> ::core::convert::From<Builder<'a>> for ::capnp::dynamic_value::Builder<'a> {
            fn from(builder: Builder<'a>) -> Self {
                Self::Struct(::capnp::dynamic_struct::Builder::new(
                    builder.builder,
                    ::capnp::schema::StructSchema::new(
                        ::capnp::introspect::RawBrandedStructSchema {
                            generic: &_private::RAW_SCHEMA,
                            field_types: _private::get_field_types,
                            annotation_types: _private::get_annotation_types,
                        },
                    ),
                ))
            }
        }

        impl<'a> ::capnp::traits::ImbueMut<'a> for Builder<'a> {
            fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {
                self.builder
                    .imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))
            }
        }

        impl<'a> ::capnp::traits::FromPointerBuilder<'a> for Builder<'a> {
            fn init_pointer(
                builder: ::capnp::private::layout::PointerBuilder<'a>,
                _size: u32,
            ) -> Self {
                builder
                    .init_struct(<Self as ::capnp::traits::HasStructSize>::STRUCT_SIZE)
                    .into()
            }
            fn get_from_pointer(
                builder: ::capnp::private::layout::PointerBuilder<'a>,
                default: ::core::option::Option<&'a [::capnp::Word]>,
            ) -> ::capnp::Result<Self> {
                ::core::result::Result::Ok(
                    builder
                        .get_struct(
                            <Self as ::capnp::traits::HasStructSize>::STRUCT_SIZE,
                            default,
                        )?
                        .into(),
                )
            }
        }

        impl<'a> ::capnp::traits::SetPointerBuilder for Reader<'a> {
            fn set_pointer_builder(
                mut pointer: ::capnp::private::layout::PointerBuilder<'_>,
                value: Self,
                canonicalize: bool,
            ) -> ::capnp::Result<()> {
                pointer.set_struct(&value.reader, canonicalize)
            }
        }

        impl<'a> Builder<'a> {
            pub fn into_reader(self) -> Reader<'a> {
                self.builder.into_reader().into()
            }
            pub fn reborrow(&mut self) -> Builder<'_> {
                Builder {
                    builder: self.builder.reborrow(),
                }
            }
            pub fn reborrow_as_reader(&self) -> Reader<'_> {
                self.builder.as_reader().into()
            }

            pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
                self.builder.as_reader().total_size()
            }
            #[inline]
            pub fn get_key(self) -> u64 {
                self.builder.get_data_field::<u64>(0)
            }
            #[inline]
            pub fn set_key(&mut self, value: u64) {
                self.builder.set_data_field::<u64>(0, value);
            }
            #[inline]
            pub fn get_value(self) -> u8 {
                self.builder.get_data_field::<u8>(8)
            }
            #[inline]
            pub fn set_value(&mut self, value: u8) {
                self.builder.set_data_field::<u8>(8, value);
            }
        }

        pub struct Pipeline {
            _typeless: ::capnp::any_pointer::Pipeline,
        }
        impl ::capnp::capability::FromTypelessPipeline for Pipeline {
            fn new(typeless: ::capnp::any_pointer::Pipeline) -> Self {
                Self {
                    _typeless: typeless,
                }
            }
        }
        impl Pipeline {}
        mod _private {
            pub static ENCODED_NODE: [::capnp::Word; 49] = [
                ::capnp::word(0, 0, 0, 0, 5, 0, 6, 0),
                ::capnp::word(235, 176, 198, 13, 10, 88, 114, 232),
                ::capnp::word(27, 0, 0, 0, 1, 0, 2, 0),
                ::capnp::word(108, 81, 44, 142, 168, 245, 252, 135),
                ::capnp::word(0, 0, 7, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(21, 0, 0, 0, 10, 1, 0, 0),
                ::capnp::word(37, 0, 0, 0, 7, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(33, 0, 0, 0, 119, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(99, 111, 109, 112, 97, 99, 116, 46),
                ::capnp::word(99, 97, 112, 110, 112, 58, 67, 111),
                ::capnp::word(109, 112, 97, 99, 116, 77, 111, 100),
                ::capnp::word(101, 108, 46, 69, 110, 116, 114, 121),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 1, 0, 1, 0),
                ::capnp::word(8, 0, 0, 0, 3, 0, 4, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 1, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(41, 0, 0, 0, 34, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(36, 0, 0, 0, 3, 0, 1, 0),
                ::capnp::word(48, 0, 0, 0, 2, 0, 1, 0),
                ::capnp::word(1, 0, 0, 0, 8, 0, 0, 0),
                ::capnp::word(0, 0, 1, 0, 1, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(45, 0, 0, 0, 50, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(40, 0, 0, 0, 3, 0, 1, 0),
                ::capnp::word(52, 0, 0, 0, 2, 0, 1, 0),
                ::capnp::word(107, 101, 121, 0, 0, 0, 0, 0),
                ::capnp::word(9, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(9, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(118, 97, 108, 117, 101, 0, 0, 0),
                ::capnp::word(6, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(6, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
                ::capnp::word(0, 0, 0, 0, 0, 0, 0, 0),
            ];
            pub fn get_field_types(index: u16) -> ::capnp::introspect::Type {
                match index {
                    0 => <u64 as ::capnp::introspect::Introspect>::introspect(),
                    1 => <u8 as ::capnp::introspect::Introspect>::introspect(),
                    _ => panic!("invalid field index {}", index),
                }
            }
            pub fn get_annotation_types(
                child_index: Option<u16>,
                index: u32,
            ) -> ::capnp::introspect::Type {
                panic!("invalid annotation indices ({:?}, {}) ", child_index, index)
            }
            pub static RAW_SCHEMA: ::capnp::introspect::RawStructSchema =
                ::capnp::introspect::RawStructSchema {
                    encoded_node: &ENCODED_NODE,
                    nonunion_members: NONUNION_MEMBERS,
                    members_by_discriminant: MEMBERS_BY_DISCRIMINANT,
                };
            pub static NONUNION_MEMBERS: &[u16] = &[0, 1];
            pub static MEMBERS_BY_DISCRIMINANT: &[u16] = &[];
            pub const TYPE_ID: u64 = 0xe872_580a_0dc6_b0eb;
        }
    }
}
