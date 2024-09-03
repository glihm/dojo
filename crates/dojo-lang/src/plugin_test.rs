use std::path::PathBuf;

use cairo_lang_compiler::db::RootDatabase;
use cairo_lang_compiler::diagnostics::get_diagnostics_as_string;
use cairo_lang_debug::debug::DebugWithDb;
use cairo_lang_defs::db::DefsGroup;
use cairo_lang_defs::ids::ModuleId;
use cairo_lang_diagnostics::DiagnosticLocation;
use cairo_lang_filesystem::cfg::{Cfg, CfgSet};
use cairo_lang_filesystem::db::FilesGroup;
use cairo_lang_filesystem::ids::Directory;
use cairo_lang_filesystem::span::{TextOffset, TextSpan, TextWidth};
use cairo_lang_project::{AllCratesConfig, ProjectConfig, ProjectConfigContent};
use cairo_lang_semantic::test_utils::setup_test_module;
use cairo_lang_starknet::starknet_plugin_suite;
use cairo_lang_test_utils::parse_test_file::{TestFileRunner, TestRunnerResult};
use cairo_lang_test_utils::{get_direct_or_file_content, verify_diagnostics_expectation};
use cairo_lang_utils::ordered_hash_map::OrderedHashMap;
use cairo_lang_utils::Upcast;
use dojo_test_utils::compiler::corelib;
use dojo_world::config::DEFAULT_NAMESPACE_CFG_KEY;

use super::dojo_plugin_suite;

#[derive(Default)]
struct ExpandContractTestRunner {}

impl TestFileRunner for ExpandContractTestRunner {
    fn run(
        &mut self,
        inputs: &OrderedHashMap<String, String>,
        args: &OrderedHashMap<String, String>,
    ) -> TestRunnerResult {
        let mut cfg_set = CfgSet::new();

        cfg_set
            .insert(Cfg { key: DEFAULT_NAMESPACE_CFG_KEY.into(), value: Some("dojo_test".into()) });

        let db = RootDatabase::builder()
            .with_project_config(ProjectConfig {
                base_path: PathBuf::from("./"),
                corelib: Some(Directory::Real(corelib())),
                content: ProjectConfigContent {
                    crate_roots: OrderedHashMap::default(),
                    crates_config: AllCratesConfig::default(),
                },
            })
            .with_cfg(cfg_set)
            .with_plugin_suite(dojo_plugin_suite())
            .with_plugin_suite(starknet_plugin_suite())
            .build()
            .unwrap();

        let (_, cairo_code) = get_direct_or_file_content(&inputs["cairo_code"]);
        let (test_module, _semantic_diagnostics) = setup_test_module(&db, &cairo_code).split();

        let mut module_ids = vec![test_module.module_id];
        if let Ok(submodules_ids) = db.module_submodules_ids(test_module.module_id) {
            module_ids.extend(submodules_ids.iter().copied().map(ModuleId::Submodule));
        }
        let mut files = vec![];
        for module_files in
            module_ids.into_iter().filter_map(|module_id| db.module_files(module_id).ok())
        {
            for file in module_files.iter().copied() {
                if !files.contains(&file) {
                    files.push(file);
                }
            }
        }
        let mut file_contents = vec![];

        for file_id in files {
            let content = db.file_content(file_id).unwrap();
            let start = TextOffset::default();
            let end = start.add_width(TextWidth::from_str(&content));
            let content_location = DiagnosticLocation { file_id, span: TextSpan { start, end } };
            let original_location = content_location.user_location(db.upcast());
            let origin = (content_location != original_location)
                .then(|| format!("{:?}\n", original_location.debug(db.upcast())))
                .unwrap_or_default();
            let file_name = file_id.file_name(&db);
            file_contents.push(format!("{origin}{file_name}:\n\n{content}"));
        }

        let diagnostics = get_diagnostics_as_string(&db, &[test_module.crate_id]);
        let error = verify_diagnostics_expectation(args, &diagnostics);

        TestRunnerResult {
            outputs: OrderedHashMap::from([
                ("generated_cairo_code".into(), file_contents.join("\n\n")),
                ("expected_diagnostics".into(), diagnostics),
            ]),
            error,
        }
    }
}

cairo_lang_test_utils::test_file_test_with_runner!(
    expand_contract,
    "src/plugin_test_data",
    {
        model: "model",
        print: "print",
        introspect: "introspect",
        system: "system",
    },
    ExpandContractTestRunner
);
