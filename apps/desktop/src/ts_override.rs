//! TypeScript 覆写转译：不依赖系统 Node/Bun，不开放模块加载。
use lanpath_common::{LanpathError, LanpathResult};
use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer};
use std::path::Path;

pub(crate) fn compile(source: &str) -> LanpathResult<String> {
    if source.trim().is_empty() {
        return Ok(String::new());
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts().with_module(false)).parse();
    if let Some(error) = parsed.diagnostics.iter().next() {
        return Err(invalid(
            error
                .clone()
                .render_with_source_code(source.to_owned())
                .to_string(),
        ));
    }
    let mut program = parsed.program;
    if program
        .body
        .iter()
        .any(|statement| statement.as_module_declaration().is_some())
    {
        return Err(invalid(
            "不支持 import/export；请定义 function main(config)".into(),
        ));
    }

    let has_main = program.body.iter().any(|statement| {
        matches!(statement,
            Statement::FunctionDeclaration(function)
                if function.id.as_ref().is_some_and(|id| id.name == "main")
                    && function.body.is_some() && !function.r#async && !function.generator
        )
    });
    if !has_main {
        return Err(invalid(
            "请定义同步 function main(config) { return config; }".into(),
        ));
    }
    let semantics = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .with_enum_eval(true)
        .build(&program);
    if let Some(error) = semantics.diagnostics.iter().next() {
        return Err(invalid(
            error
                .clone()
                .render_with_source_code(source.to_owned())
                .to_string(),
        ));
    }
    let options = TransformOptions::default();
    let transformed = Transformer::new(&allocator, Path::new("override.ts"), &options)
        .build_with_scoping(semantics.semantic.into_scoping(), &mut program);
    if let Some(error) = transformed.diagnostics.iter().next() {
        return Err(invalid(
            error
                .clone()
                .render_with_source_code(source.to_owned())
                .to_string(),
        ));
    }
    let code = Codegen::new().build(&program).code;
    lanpath_client::validate_js_override(&code).map_err(invalid)?;
    Ok(code)
}
fn invalid(error: String) -> LanpathError {
    LanpathError::Validation(format!("TypeScript 覆写：{error}"))
}

/// 共享 Profile 的 js 字段承载 TS 源文；只向共享合成器传已转译的 JS。
/// YAML 旧字段不再执行，也不会发起 YAML 远程请求。
pub(crate) async fn resolve(
    cache: &Path,
    profile: &lanpath_client::Profile,
    context: &lanpath_client::RemoteFetchContext,
) -> LanpathResult<(lanpath_client::EffectiveOverrides, Vec<String>)> {
    let mut admitted = profile.clone();
    admitted.yaml_override.clear();
    admitted.yaml_url = None;
    let (mut effective, mut warnings) =
        lanpath_client::resolve_remote_overrides_with_context(cache, &admitted, context).await;
    if !profile.yaml_override.trim().is_empty() || profile.yaml_url.is_some() {
        warnings.push("旧 YAML 覆写已停用，请改用 TypeScript 覆写".into());
    }
    effective.local_js = compile(&effective.local_js)?;
    effective.remote_js = compile(&effective.remote_js)?;
    Ok((effective, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn typed_script_runs_with_shared_executor() {
        let js = compile("type Config = { value?: number }; function main(config: Config): Config { config.value = 42; return config; }").unwrap();
        let result = lanpath_client::profile::apply_js_override(serde_json::json!({}), &js)
            .await
            .unwrap();
        assert_eq!(result["value"], 42);
    }
    #[tokio::test]
    async fn enum_values_survive_type_erasure() {
        let js = compile("const enum Level { Quiet = 'error' }; function main(config: any) { config.level = Level.Quiet; return config; }").unwrap();
        let result = lanpath_client::profile::apply_js_override(serde_json::json!({}), &js)
            .await
            .unwrap();
        assert_eq!(result["level"], "error");
    }
    #[test]
    fn invalid_typescript_and_module_imports_are_rejected() {
        assert!(compile("function main(c: ) { return c; }").is_err());
        assert!(compile("import x from 'fs'; function main(c) { return c; }").is_err());
        assert!(compile("const unrelated = 1").is_err());
        assert!(compile("// function main(c) {}\nconst value = 1;").is_err());
        assert!(compile("async function main(c: any) { return c; }").is_err());
    }
    #[tokio::test]
    async fn remote_then_local_and_yaml_is_never_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let profile = lanpath_client::Profile {
            id: uuid::Uuid::new_v4(),
            name: "TS".into(),
            yaml_override: "ignored: true".into(),
            yaml_url: Some("http://127.0.0.1:1/never".into()),
            js_override: "function main(c: any) { c.value = 2; return c; }".into(),
            js_url: None,
        };
        let (effective, warnings) = resolve(
            dir.path(),
            &profile,
            &lanpath_client::RemoteFetchContext::cache_only(),
        )
        .await
        .unwrap();
        assert!(effective.remote_yaml.is_empty());
        assert!(effective.local_yaml.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(effective.local_js.contains("c.value = 2"));
    }
}
