use shader_sense::symbols::symbols::{ShaderSymbol, ShaderSymbolList};

use super::GlslIntrinsicParser;

impl GlslIntrinsicParser {
    #[allow(dead_code)]
    fn get_glsl_ext_mesh_shader(&self) -> ShaderSymbolList {
        // https://github.com/KhronosGroup/GLSL/blob/main/extensions/ext/GLSL_EXT_mesh_shader.txt
        let mut list = ShaderSymbolList::default();
        #[allow(unreachable_code)]
        list.constants.push(ShaderSymbol {
            label: "gl_PrimitivePointIndicesEXT".into(),
            description: todo!(),
            version: todo!(),
            stages: todo!(),
            link: todo!(),
            data: todo!(),
            range: None,
            scope: None,
            scope_stack: None,
        });
        list
    }
}
