use shader_sense::{
    shader::ShaderStage,
    symbols::symbols::{
        ShaderMethod, ShaderParameter, ShaderSignature, ShaderSymbol, ShaderSymbolData,
        ShaderSymbolList,
    },
};

use super::HlslIntrinsicParser;

pub fn new_hlsl_scalar(label: &str, description: &str, version: &str) -> ShaderSymbol {
    ShaderSymbol {
        label: label.into(),
        description: description.into(),
        version: version.to_string(),
        stages: vec![],
        link: Some(
            "https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-scalar"
                .into(),
        ),
        data: ShaderSymbolData::Types {
            constructors: vec![ShaderSignature {
                returnType: "".into(),
                description: format!("Constructor for type {}", label),
                parameters: vec![ShaderParameter {
                    ty: label.into(),
                    label: "value".into(),
                    count: None,
                    description: "".into(),
                    range: None,
                }],
            }],
        },
        scope: None,
        range: None,
        scope_stack: None,
    }
}

impl HlslIntrinsicParser {
    pub fn add_types(&self, symbols: &mut ShaderSymbolList) {
        fn get_texture_object_methods(context: &str) -> Vec<ShaderMethod> {
            // https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texture2d
            // https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-to-gather
            let mut methods = Vec::new();
            match context {
                "Texture2D" | "Texture2DArray" | "TextureCube" | "TextureCubeArray" => {
                    let mut method = ShaderMethod {
                        context: context.into(),
                        label: "Gather".into(),
                        signature: ShaderSignature {
                            returnType: "void".into(),
                            description: "Gets the four samples (red component only) that would be used for bilinear interpolation when sampling a texture.".into(),
                            parameters: vec![
                                ShaderParameter {
                                    ty: "sampler".into(),
                                    label: "s".into(),
                                    description: "A Sampler state. This is an object declared in an effect file that contains state assignments.".into(),
                                    count: None,
                                    range: None,
                                },
                                ShaderParameter {
                                    ty: match context {
                                        "Texture2D" => "float2",
                                        "Texture2DArray" | "TextureCube" => "float3",
                                        "TextureCubeArray" => "float4",
                                        _ => unreachable!(),
                                    }.into(),
                                    label: "location".into(),
                                    description: "The texture coordinates. The argument type is dependent on the texture-object type. ".into(),
                                    count: None,
                                    range: None,
                                }
                            ]
                        },
                        range: None,
                    };
                    if context == "Texture2D" || context == "Texture2DArray" {
                        method.signature.parameters.push(ShaderParameter {
                            ty: match context {
                                "Texture2D" | "Texture2DArray" => "int2",
                                _ => unreachable!(),
                            }.into(),
                            label: "offset".into(),
                            description: "An optional texture coordinate offset, which can be used for any texture-object type; the offset is applied to the location before sampling. The argument type is dependent on the texture-object type. For shaders targeting Shader Model 5.0 and above, the 6 least significant bits of each offset value is honored as a signed value, yielding [-32..31] range. For previous shader model shaders, offsets need to be immediate integers between -8 and 7.".into(),
                            count: None,
                            range: None,
                        });
                    }
                    methods.push(method);
                }
                _ => {} // not supported
            }
            // TODO: https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-to-getdimensions
            let (dimensions, has_layers) = match context {
                "Texture1D" => (2, false),
                "Texture1DArray" => (2, true),
                "Texture2D" => (2, false),
                "Texture2DArray" => (2, true),
                "Texture3D" => (2, false),
                "Texture3DArray" => (2, true),
                "Texture2DMS" => (2, false),
                "Texture2DMSArray" => (2, true),
                "TextureCube" => (2, true),
                "TextureCubeArray" => (2, true),
                _ => unreachable!(),
            };
            methods.push(ShaderMethod {
                context: context.into(),
                label: "GetDimensions".into(),
                signature: ShaderSignature {
                    returnType: "void".into(),
                    description: "".into(),
                    parameters: vec![ShaderParameter {
                        ty: "uint".into(),
                        label: "dim".into(),
                        description: "The length, in bytes, of the buffer.".into(),
                        count: None,
                        range: None,
                    }],
                },
                range: None,
            });
            match context {
                "Texture2DMS" | "Texture2DMSArray" => methods.push(ShaderMethod {
                    context: context.into(),
                    label: "GetSamplePosition".into(),
                    signature: ShaderSignature {
                        returnType: "float2".into(),
                        description: "Gets the position of the specified sample.".into(),
                        parameters: vec![ShaderParameter {
                            ty: "sampler".into(),
                            label: "s".into(),
                            description: "The zero-based sample index.".into(),
                            count: None,
                            range: None,
                        }],
                    },
                    range: None,
                }),
                _ => {}
            }
            if context != "TextureCube" && context != "TextureCubeArray" {
                // Load
                let mut method = ShaderMethod {
                    context: context.into(),
                    label: "Load".into(),
                    signature: ShaderSignature {
                        returnType: "float4".into(),
                        description: "Reads texel data without any filtering or sampling.".into(),
                        parameters: vec![
                            ShaderParameter {
                                ty: match context {
                                    "Buffer" => "int",
                                    "Texture1D" | "Texture2DMS" => "int2",
                                    "Texture1DArray" | "Texture2D" | "Texture2DMSArray" => "int3",
                                    "Texture2DArray" | "Texture3D" => "int4",
                                    str => unreachable!("Reached {}", str)
                                }.into(),
                                label: "location".into(),
                                description: "The texture coordinates; the last component specifies the mipmap level. This method uses a 0-based coordinate system and not a 0.0-1.0 UV system. The argument type is dependent on the texture-object type.".into(),
                                count: None,
                                range: None,
                            }
                        ]
                    },
                    range: None,
                };
                match context {
                    "Texture2DMS" | "Texture2DMSArray" => method.signature.parameters.push(ShaderParameter {
                        ty: "int".into(),
                        label: "sampleIndex".into(),
                        description: "A sampling index. Required for multi-sample textures. Not supported for other textures.".into(),
                        count: None,
                        range: None,
                    }),
                    _ => {}
                }
                method.signature.parameters.push(ShaderParameter {
                    ty: match context {
                        "Texture1D" | "Texture1DArray" => "int",
                        "Texture2D" | "Texture2DArray" | "Texture2DMS" | "Texture2DMSArray" => "int2",
                        "Texture3D" => "int3",
                        _ => unreachable!()
                    }.into(),
                    label: "sampleIndex".into(),
                    description: "A sampling index. Required for multi-sample textures. Not supported for other textures.".into(),
                    count: None,
                    range: None,
                });
                methods.push(method);
            }
            if context != "Texture2DMS" && context != "Texture2DMSArray" {
                // Sample | SampleBias | SampleCmp | SampleCmpLevelZero | SampleGrad | SampleLevel.
                let variants = vec![
                    ("Sample", "Samples a texture."),
                    ("SampleBias", "Samples a texture, after applying the input bias to the mipmap level."), // Add a bias before offset
                    ("SampleCmp", "Samples a texture and compares a single component against the specified comparison value."), // Add a CompareValue before offset
                    ("SampleCmpLevelZero", "Samples a texture and compares the result to a comparison value. This function is identical to calling SampleCmp on mipmap level 0 only."), // Add a CompareValue before offset
                    ("SampleGrad", "Samples a texture using a gradient to influence the way the sample location is calculated."), // Add a CompareValue before offset
                    ("SampleLevel", "Samples a texture using a mipmap-level offset. This function is similar to Sample except that it uses the LOD level (in the last component of the location parameter) to choose the mipmap level. For example, a 2D texture uses the first two components for uv coordinates and the third component for the mipmap level."), // Add a lod before offset
                ];
                for (variant, variant_description) in variants {
                    let mut method = ShaderMethod {
                        context: context.into(),
                        label: variant.into(),
                        signature: ShaderSignature {
                            returnType: "float4".into(),
                            description:variant_description.into(),
                            parameters: vec![
                                ShaderParameter {
                                    ty: "sampler".into(),
                                    label: "s".into(),
                                    description: "A Sampler state. This is an object declared in an effect file that contains state assignments.".into(),
                                    count: None,
                                    range: None,
                                },
                                ShaderParameter {
                                    ty: match context {
                                        "Texture1D" => "float",
                                        "Texture1DArray" | "Texture2D" => "float2",
                                        "Texture2DArray" | "Texture3D" | "TextureCube" => "float3",
                                        "TextureCubeArray" => "float4",
                                        _ => unreachable!()
                                    }.into(),
                                    label: "location".into(),
                                    description: "The texture coordinates. The argument type is dependent on the texture-object type. If the texture object is an array, the last component is the array index.".into(),
                                    count: None,
                                    range: None,
                                },
                            ]
                        },
                        range: None,
                    };
                    match context {
                        "SampleBias" => method.signature.parameters.push(ShaderParameter {
                            ty: "float".into(),
                            label: "bias".into(),
                            description: "The bias value, which is a floating-point number between -16.0 and 15.99, is applied to a mip level before sampling.".into(),
                            count: None,
                            range: None,
                        }),
                        "SampleCmp" | "SampleCmpLevelZero" => method.signature.parameters.push(ShaderParameter {
                            ty: "float".into(),
                            label: "CompareValue".into(),
                            description: "A floating-point value to use as a comparison value.".into(),
                            count: None,
                            range: None,
                        }),
                        "SampleGrad" => {
                            method.signature.parameters.push(ShaderParameter {
                                ty: match context {
                                    "Texture1D" |  "Texture1DArray" => "float",
                                    "Texture2D" | "Texture2DArray" => "float2",
                                    "Texture3D" | "TextureCubeArray" | "TextureCube" => "float3",
                                    _ => unreachable!()
                                }.into(),
                                label: "DDX".into(),
                                description: "The rate of change of the surface geometry in the x direction. The argument type is dependent on the texture-object type.".into(),
                                count: None,
                                range: None,
                            });
                            method.signature.parameters.push(ShaderParameter {
                                ty: match context {
                                    "Texture1D" |  "Texture1DArray" => "float",
                                    "Texture2D" | "Texture2DArray" => "float2",
                                    "Texture3D" | "TextureCubeArray" | "TextureCube" => "float3",
                                    _ => unreachable!()
                                }.into(),
                                label: "DDY".into(),
                                description: "The rate of change of the surface geometry in the y direction. The argument type is dependent on the texture-object type.".into(),
                                count: None,
                                range: None,
                            });
                        },
                        "SampleLevel" => method.signature.parameters.push(ShaderParameter {
                            ty: "float".into(),
                            label: "LOD".into(),
                            description: "A number that specifies the mipmap level (internally clamped to the smallest map level). If the value is = 0, the zero'th (biggest map) is used. The fractional value (if supplied) is used to interpolate between two mipmap levels.".into(),
                            count: None,
                            range: None,
                        }),
                        _ => {} // Nothing
                    }
                    match context {
                        "TextureCube" | "TextureCubeArray" => {} // not supported
                        _ => method.signature.parameters.push(ShaderParameter {
                            ty: match context {
                                "Texture1D" | "Texture1DArray" => "int",
                                "Texture2D" | "Texture2DArray" => "int2",
                                "Texture3D" => "int3",
                                _ => unreachable!()
                            }.into(),
                            label: "offset".into(),
                            description: "An optional texture coordinate offset, which can be used for any texture-object type; the offset is applied to the location before sampling. The texture offsets need to be static. The argument type is dependent on the texture-object type. For more info, see Applying texture coordinate offsets.".into(),
                            count: None,
                            range: None,
                        }),
                    }
                    methods.push(method);
                }
            }
            methods
        }
        fn get_buffer_object_methods() -> Vec<ShaderMethod> {
            vec![] // Load
        }
        // sm 4.0 : Object<Type, Samples> name
        // https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-to-type
        symbols.types.push(ShaderSymbol {
            label: "Buffer".into(),
            description: "".into(),
            version: "sm4".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-buffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "Texture1D".into(),
            description: "".into(),
            version: "sm4".into(),
            stages: vec![],
            link: Some(
                "https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texture1d"
                    .into(),
            ),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("Texture1D"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "Texture1DArray".into(),
            description: "".into(),
            version: "sm4".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texture1darray".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("Texture1DArray"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "Texture2D".into(),
            description: "".into(),
            version: "sm4".into(),
            stages: vec![],
            link: Some(
                "https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texture2d"
                    .into(),
            ),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("Texture2D"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "Texture2DArray".into(),
            description: "".into(),
            version: "sm4".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texture2darray".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("Texture2DArray"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "Texture3D".into(),
            description: "".into(),
            version: "sm4".into(),
            stages: vec![],
            link: Some(
                "https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texture3d"
                    .into(),
            ),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("Texture3D"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "TextureCube".into(),
            description: "".into(),
            version: "sm4".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texturecube".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("TextureCube"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "TextureCubeArray".into(),
            description: "".into(),
            version: "sm4.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texturecubearray".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("TextureCubeArray"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "Texture2DMS".into(),
            description: "".into(),
            version: "sm4.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-texture2dms".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("Texture2DMS"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "Texture2DMSArray".into(),
            description: "".into(),
            version: "sm4.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-Texture2DMSArray".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: get_texture_object_methods("Texture2DMSArray"),
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        // sm 5.0 : Object<Type, Samples> name
        // https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/d3d11-graphics-reference-sm5-objects
        symbols.types.push(ShaderSymbol {
            label: "AppendStructuredBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-AppendStructuredBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![/*ShaderMethod {
                    // GetDimensions
                    // Load
                    // Operator[]
                }*/],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "ByteAddressBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-ByteAddressBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "ByteAddressBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-ByteAddressBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "ConsumeStructuredBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-ConsumeStructuredBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "InputPatch".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![ShaderStage::TesselationControl],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-InputPatch".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "OutputPatch".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![ShaderStage::TesselationControl],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-OutputPatch".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some(
                "https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWBuffer"
                    .into(),
            ),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWByteAddressBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWByteAddressBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWStructuredBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWStructuredBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWTexture1D".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWTexture1D".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWTexture1DArray".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWTexture1DArray".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWTexture2D".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWTexture2D".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWTexture2DArray".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWTexture2DArray".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RWTexture3D".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-RWTexture3D".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "StructuredBuffer".into(),
            description: "".into(),
            version: "sm5".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-StructuredBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        // sm 5.1
        symbols.types.push(ShaderSymbol {
            label: "StructuredBuffer".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-StructuredBuffer".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedBuffer".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedByteAddressBuffer".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedStructuredBuffer".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedTexture1D".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedTexture1DArray".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedTexture2D".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedTexture2DArray".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });
        symbols.types.push(ShaderSymbol {
            label: "RasterizerOrderedTexture3D".into(),
            description: "".into(),
            version: "sm5.1".into(),
            stages: vec![],
            link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/shader-model-5-1-objects".into()),
            data: ShaderSymbolData::Struct {
                constructors: vec![],
                members: vec![],
                methods: vec![],
            },
            scope_stack: None,
            range: None,
            scope: None,
        });

        // Manually push types as they are not in documentation
        let mut scalar_types = Vec::new();
        scalar_types.push(new_hlsl_scalar(
            "bool",
            "conditional type, values may be either true or false",
            "",
        ));
        scalar_types.push(new_hlsl_scalar("int", "32-bit signed integer", ""));
        scalar_types.push(new_hlsl_scalar("uint", "32-bit unsigned integer", ""));
        scalar_types.push(new_hlsl_scalar("dword", "32-bit unsigned integer", ""));
        scalar_types.push(new_hlsl_scalar("half", "16-bit floating point value", ""));
        scalar_types.push(new_hlsl_scalar("float", "32-bit floating point value", ""));
        scalar_types.push(new_hlsl_scalar(
            "double",
            "64-bit floating point value.",
            "",
        ));
        // Minimum are only supported with windows 8+
        scalar_types.push(new_hlsl_scalar(
            "min16float",
            "minimum 16-bit floating point value. Only supported on Windows 8+ only.",
            "",
        ));
        scalar_types.push(new_hlsl_scalar(
            "min10float",
            "minimum 10-bit floating point value. Only supported on Windows 8+ only.",
            "",
        ));
        scalar_types.push(new_hlsl_scalar(
            "min16int",
            "minimum 16-bit signed integer. Only supported on Windows 8+ only.",
            "",
        ));
        scalar_types.push(new_hlsl_scalar(
            "min12int",
            "minimum 12-bit signed integer. Only supported on Windows 8+ only.",
            "",
        ));
        scalar_types.push(new_hlsl_scalar(
            "min16uint",
            "minimum 16-bit unsigned integer. Only supported on Windows 8+ only.",
            "",
        ));
        scalar_types.push(new_hlsl_scalar(
            "uint64_t",
            "A 64-bit unsigned integer.",
            "sm6",
        ));
        scalar_types.push(new_hlsl_scalar(
            "int64_t",
            "A 64-bit signed integer.",
            "sm6",
        ));
        // TODO: -enable16bnit float16_t + uint16_t
        fn get_vector_component_label(index: u32) -> String {
            match index {
                0 => "x".into(),
                1 => "y".into(),
                2 => "z".into(),
                3 => "w".into(),
                _ => unreachable!(""),
            }
        }
        fn get_matrix_component_label(index_col: u32, index_row: u32) -> String {
            format!("m{}{}", index_col, index_row)
        }
        for component_col in 1..=4 {
            // Vectors
            for scalar in &scalar_types {
                let fmt = format!("{}{}", scalar.label, component_col);
                symbols.types.push(ShaderSymbol {
                    label: fmt.clone(),
                    description: format!("Vector with {} components of {}", component_col, scalar.label),
                    link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-vector".into()),
                    data: ShaderSymbolData::Types { constructors: vec![
                        ShaderSignature {
                            returnType: fmt.clone(),
                            description: format!("Constructor for type {}", fmt),
                            parameters: vec![ShaderParameter {
                                ty: fmt.clone(),
                                label: "value".into(),
                                count: None,
                                description: "".into(),
                                range:None,
                            }],
                        },
                        ShaderSignature {
                            returnType: fmt.clone(),
                            description: format!("Constructor for type {}", fmt),
                            parameters: (0..component_col).map(|parameter_index| ShaderParameter {
                                ty: scalar.label.clone(),
                                label: get_vector_component_label(parameter_index),
                                count: None,
                                description: "".into(),
                                range:None,
                            }).collect(),
                        }
                    ]},
                    version: "".into(),
                    stages: vec![],
                    range: None,
                    scope: None,
                    scope_stack:None,
                });
                /*symbols.types.push(ShaderSymbol {
                    label: format!("vector<{},{}>", scalar.label, component_col),
                    description: format!("Vector with {} components of {}", component_col, scalar.label),
                    link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-vector".into()),
                    data: ShaderSymbolData::Types { ty:fmt },
                    version: "".into(),
                    stages: vec![]
                });*/
                for component_row in 1..=4 {
                    let fmt = format!("{}{}x{}", scalar.label, component_row, component_col);
                    symbols.types.push(ShaderSymbol{
                        label: fmt.clone(),
                        description: format!("Matrice with {} rows and {} columns of {}", component_row, component_col, scalar.label),
                        link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-matrix".into()),
                        data: ShaderSymbolData::Types { constructors: vec![
                            ShaderSignature {
                                returnType: fmt.clone(),
                                description: format!("Constructor for type {}", fmt),
                                parameters: vec![ShaderParameter {
                                    ty: fmt.clone(),
                                    label: "value".into(),
                                    count: None,
                                    description: "".into(),
                                    range:None,
                                }],
                            },
                            ShaderSignature {
                                returnType: fmt.clone(),
                                description: format!("Constructor for type {}", fmt),
                                parameters: (0..component_col).map(|col_index|
                                    (0..component_row).map(|row_index| ShaderParameter {
                                        ty: scalar.label.clone(),
                                        label: get_matrix_component_label(col_index, row_index),
                                        count: None,
                                        description: "".into(),
                                        range:None,
                                    }).collect::<Vec<ShaderParameter>>()
                                ).collect::<Vec<Vec<ShaderParameter>>>().concat(),
                            }
                        ] },
                        version: "".into(),
                        stages: vec![],
                        range: None,
                        scope: None,
                        scope_stack:None,
                    });
                    /*symbols.types.push(ShaderSymbol{
                        label: format!("matrix<{},{},{}>", scalar.label, component_row, component_col),
                        description: format!("Matrice with {} rows and {} columns of {}", component_row, component_col, scalar.label),
                        link: Some("https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-matrix".into()),
                        data: ShaderSymbolData::Types { ty:fmt },
                        version: "".into(),
                        stages: vec![]
                    });*/
                }
            }
        }
        symbols.types.append(&mut scalar_types);
    }
}
