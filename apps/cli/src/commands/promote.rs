//! `rimmerge promote`: "promote to user rule" — copies an imported pair or
//! placement rule to a `UserDecision`-owned one that survives both import
//! toggles and a re-import.

use anyhow::{Context, bail};
use clap::Args;
use rim_analyzer::domain::ModId;
use rim_session::RuleKey;
use rim_session::use_cases::PromoteImportedRule;

use crate::common::{PathsArgs, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct PromoteArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Promote an imported pair rule: "after,before" mod ids. Mutually
    /// exclusive with `--placement`.
    #[arg(long, value_name = "AFTER,BEFORE")]
    pair: Option<String>,
    /// Promote an imported placement rule pinning this mod id. Mutually
    /// exclusive with `--pair`.
    #[arg(long, value_name = "MOD_ID")]
    placement: Option<String>,
}

fn parse_key(args: &PromoteArgs) -> anyhow::Result<RuleKey> {
    match (&args.pair, &args.placement) {
        (Some(_), Some(_)) => bail!("--pair and --placement are mutually exclusive"),
        (None, None) => bail!("one of --pair or --placement is required"),
        (Some(pair), None) => {
            let (after, before) = pair
                .split_once(',')
                .with_context(|| format!("--pair {pair:?} must be \"after,before\""))?;
            Ok(RuleKey::Pair {
                after: ModId::new(after.trim()),
                before: ModId::new(before.trim()),
            })
        }
        (None, Some(mod_id)) => Ok(RuleKey::Placement {
            mod_id: ModId::new(mod_id.trim()),
        }),
    }
}

pub fn run(args: &PromoteArgs) -> anyhow::Result<()> {
    let key = parse_key(args)?;
    let project_paths = resolve_paths(&args.paths)?;
    let mut session = build_session(project_paths)?;

    let use_case = PromoteImportedRule::new(rim_io::JsonRuleStore::new());
    let promoted = use_case
        .execute(&mut session, &key)
        .context("promoting the rule")?;

    if promoted {
        println!("promoted to a user-owned rule that survives both import toggles and a re-import");
    } else {
        println!(
            "nothing to promote: no matching imported rule, or a user-owned copy already exists"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pair: Option<&str>, placement: Option<&str>) -> PromoteArgs {
        PromoteArgs {
            paths: PathsArgs {
                game_dir: None,
                workshop_dir: None,
                mods_config: None,
                profile_dir: None,
            },
            pair: pair.map(str::to_string),
            placement: placement.map(str::to_string),
        }
    }

    #[test]
    fn parses_a_pair_key() {
        let key = parse_key(&args(Some("a,b"), None)).expect("must parse");
        assert_eq!(
            key,
            RuleKey::Pair {
                after: ModId::new("a"),
                before: ModId::new("b"),
            }
        );
    }

    #[test]
    fn parses_a_placement_key() {
        let key = parse_key(&args(None, Some("a"))).expect("must parse");
        assert_eq!(
            key,
            RuleKey::Placement {
                mod_id: ModId::new("a"),
            }
        );
    }

    #[test]
    fn rejects_both_pair_and_placement() {
        assert!(parse_key(&args(Some("a,b"), Some("c"))).is_err());
    }

    #[test]
    fn rejects_neither_pair_nor_placement() {
        assert!(parse_key(&args(None, None)).is_err());
    }

    #[test]
    fn rejects_a_pair_without_a_comma() {
        assert!(parse_key(&args(Some("a"), None)).is_err());
    }
}
