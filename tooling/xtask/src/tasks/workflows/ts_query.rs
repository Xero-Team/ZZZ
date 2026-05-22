use gh_workflow::{Step, Run, Use};
use indoc::formatdoc;

use crate::tasks::workflows::steps::named;

const TS_QUERY_LS_FILE: &str = "ts_query_ls-x86_64-unknown-linux-gnu.tar.gz";
const CI_TS_QUERY_RELEASE: &str = "tags/v3.15.1";

pub(crate) fn fetch_ts_query_ls() -> Step<Use> {
    named::uses(
        "dsaltares",
        "fetch-gh-release-asset",
        "aa37ae5c44d3c9820bc12fe675e8670ecd93bd1c",
    ) // v1.1.1
    .add_with(("repo", "ribru17/ts_query_ls"))
    .add_with(("version", CI_TS_QUERY_RELEASE))
    .add_with(("file", TS_QUERY_LS_FILE))
}

pub(crate) fn run_ts_query_ls() -> Step<Run> {
    named::bash(formatdoc!(
        r#"tar -xf "$GITHUB_WORKSPACE/{TS_QUERY_LS_FILE}" -C "$GITHUB_WORKSPACE"
        "$GITHUB_WORKSPACE/ts_query_ls" format --check . || {{
            echo "Found unformatted queries, please format them with ts_query_ls."
            echo "For easy use, install the Tree-sitter query extension:"
            echo "zed://extension/tree-sitter-query"
            false
        }}"#
    ))
}
