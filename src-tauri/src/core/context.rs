//! Foreground executable to content-context resolution.

use anyhow::Result;

use crate::data::db::{self, Context, Db};

pub fn resolve_context(db: &Db, executable: &str, domain: Option<&str>) -> Result<Context> {
    // App bundles/installers commonly replace their executable name on every
    // update. Refresh a target lazily on the dictation path so users do not
    // need to reopen the Contexts screen after a nightly release changes.
    let installed_apps = crate::system::apps::list_installed_apps_cached();
    db::reconcile_context_targets(db, &installed_apps)?;
    db::resolve_context_for_target(db, executable, domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_matches_executables_case_insensitively() {
        let db = db::open(":memory:").expect("db");
        let context = db::insert_context_returning(&db, "Editor", None, None, None, None, false)
            .expect("context");
        db::assign_context_target(&db, context.id, "editor.exe").expect("target");

        assert_eq!(
            resolve_context(&db, "EDITOR.EXE", None).unwrap().id,
            context.id
        );
    }

    #[test]
    fn resolver_uses_everywhere_for_unknown_executable() {
        let db = db::open(":memory:").expect("db");

        let context = resolve_context(&db, "unknown.exe", None).expect("fallback");
        assert_eq!(context.id, db::EVERYWHERE_CONTEXT_ID);
        assert!(context.is_everywhere);
    }
}
