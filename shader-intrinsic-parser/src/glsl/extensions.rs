use shader_sense::symbols::{
    symbol_list::ShaderSymbolList,
    symbols::{
        GlslRequirementParameter, RequirementParameter, ShaderSymbol, ShaderSymbolIntrinsic,
        ShaderSymbolMode,
    },
};

use super::GlslIntrinsicParser;

impl GlslIntrinsicParser {
    #[allow(dead_code)]
    fn get_glsl_ext_mesh_shader(&self) -> ShaderSymbolList {
        // https://github.com/KhronosGroup/GLSL/blob/main/extensions/ext/GLSL_EXT_mesh_shader.txt
        let mut list = ShaderSymbolList::default();
        #[allow(unreachable_code)]
        list.constants.push(ShaderSymbol {
            label: "gl_PrimitivePointIndicesEXT".into(),
            mode: ShaderSymbolMode::Intrinsic(ShaderSymbolIntrinsic::new(todo!(), Some(todo!()))),
            requirement: Some(RequirementParameter::Glsl(GlslRequirementParameter {
                extension: Some("GLSL_EXT_mesh_shader".into()),
                ..Default::default()
            })),
            data: todo!(),
        });
        list
    }
}
