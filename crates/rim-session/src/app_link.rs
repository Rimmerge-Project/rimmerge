//! [`AppLinkTarget`]: the app's own fixed, non-mod-specific external
//! links — the GitHub repository, its issues page, and the project's
//! support/donation page. Each maps to one constant URL, never read from
//! a file or built from any request input — the interface layer's
//! `open_app_link` command takes only a target, never a URL, the same
//! "the frontend never sends a URL" contract [`crate::mod_info::ExternalUrl`]
//! enforces for mod links.

use crate::mod_info::ExternalUrl;

/// One of the app's own fixed external links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppLinkTarget {
    /// The project's GitHub repository.
    GithubRepo,
    /// The project's GitHub issues page.
    GithubIssues,
    /// The project's support/donation page.
    Support,
}

impl AppLinkTarget {
    /// This target's fixed URL.
    #[must_use]
    pub fn url(self) -> ExternalUrl {
        ExternalUrl::from_static(match self {
            Self::GithubRepo => "https://github.com/Rimmerge-Project/rimmerge",
            Self::GithubIssues => "https://github.com/Rimmerge-Project/rimmerge/issues",
            Self::Support => "https://buymeacoffee.com/nephilim",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every target's own constant URL is a genuinely valid `http`/
    /// `https` URL with a host — proven by round-tripping it back through
    /// [`ExternalUrl::parse`]'s own validation, so a typo'd constant
    /// (missing scheme, empty host) fails this test rather than silently
    /// shipping an opener call that always errors.
    #[test]
    fn every_target_url_is_a_valid_http_or_https_url() {
        for target in [
            AppLinkTarget::GithubRepo,
            AppLinkTarget::GithubIssues,
            AppLinkTarget::Support,
        ] {
            let url = target.url();
            assert!(
                ExternalUrl::parse(url.as_str()).is_some(),
                "{target:?}'s URL {:?} must itself be a valid http(s) URL",
                url.as_str()
            );
        }
    }

    #[test]
    fn targets_have_the_expected_urls() {
        assert_eq!(
            AppLinkTarget::GithubRepo.url().as_str(),
            "https://github.com/Rimmerge-Project/rimmerge"
        );
        assert_eq!(
            AppLinkTarget::GithubIssues.url().as_str(),
            "https://github.com/Rimmerge-Project/rimmerge/issues"
        );
        assert_eq!(
            AppLinkTarget::Support.url().as_str(),
            "https://buymeacoffee.com/nephilim"
        );
    }
}
