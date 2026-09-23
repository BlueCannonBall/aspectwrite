use crate::{parser, render};
use base64::Engine;
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenderRequest {
    #[schemars(description = "ASCII LaTeX in the supported subset (maximum 8192 bytes)")]
    latex: String,
    #[schemars(description = "Optional reproducible variant selection seed")]
    seed: Option<u64>,
}

#[derive(Clone)]
pub struct AspectServer {
    router: ToolRouter<Self>,
    profile: std::sync::Arc<render::Handwriting>,
}
impl AspectServer {
    pub fn new(path: &Path) -> Result<Self, render::RenderError> {
        Ok(Self {
            router: Self::tool_router(),
            profile: std::sync::Arc::new(render::Handwriting::load(path)?),
        })
    }
}
#[tool_router]
impl AspectServer {
    #[tool(
        description = "Render a CLOSED ASCII LaTeX subset (not arbitrary TeX) as handwritten PNG. Valid: C_{10}H_{18}O\\xrightarrow{H^{+},\\,\\Delta}C_{10}H_{16}+H_2O; \\text{words}, \\mathrm{H_2O}, \\frac{a}{b}, scripts, Greek commands. Labeled arrows take exactly one {label}, never [below]. No preamble, $, packages, \\ce, \\SI or \\dfrac. Full whitelist: docs/latex-subset.md. In aligned blocks, if any row uses &, every row must; use &\\text{...} for prose in the right column. Errors include byte offsets."
    )]
    fn render_latex(&self, Parameters(req): Parameters<RenderRequest>) -> CallToolResult {
        let result = (|| {
            if req.latex.len() > 8192 {
                return Err("LaTeX exceeds 8192 bytes".to_owned());
            }
            let ast = parser::parse(&req.latex).map_err(|e| e.to_string())?;
            let png = render::png_with_seed(&ast, &self.profile, req.seed.unwrap_or(0))
                .map_err(|e| e.to_string())?;
            Ok(png)
        })();
        match result {
            Ok(png) => CallToolResult::success(vec![ContentBlock::image(
                base64::engine::general_purpose::STANDARD.encode(png),
                "image/png",
            )]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
        }
    }
}
#[tool_handler(router = self.router)]
impl ServerHandler for AspectServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Render LaTeX with the configured handwriting profile")
    }
}
