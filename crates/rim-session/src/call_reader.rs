//! [`CallReader`]: what one use-case call reads def sources through.

use rim_analyzer::domain::XmlLocator;

use crate::ports::{DefSourceCallView, DefSourceError, DefSourceReader, ElementExpectation};

/// The reader one use-case call reads through: the reader's own call view
/// ([`DefSourceReader::call_view`]) when it offers one, else the reader
/// itself. Opened at the start of a call and dropped when it returns, so
/// whatever the view trusts lasts exactly one call.
pub(crate) enum CallReader<'a, R: ?Sized> {
    /// The reader's own view for this call.
    View(DefSourceCallView<'a>),
    /// A reader with no view, read directly.
    Direct(&'a R),
}

impl<'a, R: DefSourceReader + ?Sized> CallReader<'a, R> {
    /// Opens `reader` for one call.
    pub(crate) fn open(reader: &'a R) -> Self {
        reader.call_view().map_or(Self::Direct(reader), Self::View)
    }
}

/// Reads through the view or the reader. `call_view` keeps its default
/// (`None`): a use case nested inside a call reads through the outer
/// call's view instead of opening a second one.
impl<R: DefSourceReader + ?Sized> DefSourceReader for CallReader<'_, R> {
    fn read_element(
        &self,
        locator: &XmlLocator,
        expected: &ElementExpectation,
    ) -> Result<String, DefSourceError> {
        match self {
            Self::View(view) => view.read_element(locator, expected),
            Self::Direct(reader) => reader.read_element(locator, expected),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use crate::test_support::{CallCountingReader, InMemoryDefSourceReader};

    use super::*;

    fn locator() -> XmlLocator {
        XmlLocator::new(Arc::from(Path::new("Defs/a.xml")), vec![0])
    }

    fn expectation() -> ElementExpectation {
        ElementExpectation {
            tag: "ThingDef".to_string(),
            def_name: Some("A".to_string()),
            name_attr: None,
        }
    }

    fn seeded() -> InMemoryDefSourceReader {
        InMemoryDefSourceReader::new(std::collections::BTreeMap::from([(
            locator(),
            "<ThingDef><defName>A</defName></ThingDef>".to_string(),
        )]))
    }

    #[test]
    fn a_reader_with_a_call_view_is_read_through_that_view() {
        let reader = CallCountingReader::new(seeded());

        let call = CallReader::open(&reader);
        let text = call.read_element(&locator(), &expectation());

        assert_eq!(
            text.as_deref(),
            Ok("<ThingDef><defName>A</defName></ThingDef>")
        );
        assert_eq!(reader.views_opened(), 1);
        assert_eq!(reader.reads_through_views(), 1);
        assert_eq!(reader.direct_reads(), 0);
    }

    #[test]
    fn a_reader_behind_an_arc_still_opens_its_call_view() {
        let reader = Arc::new(CallCountingReader::new(seeded()));
        let shared: Arc<dyn DefSourceReader + Send + Sync> = reader.clone();

        CallReader::open(&shared)
            .read_element(&locator(), &expectation())
            .expect("read");

        assert_eq!(reader.views_opened(), 1);
        assert_eq!(reader.direct_reads(), 0);
    }

    #[test]
    fn a_reader_without_a_call_view_is_read_directly() {
        let reader = seeded();

        let call = CallReader::open(&reader);

        assert!(matches!(call, CallReader::Direct(_)));
        assert!(call.read_element(&locator(), &expectation()).is_ok());
    }

    #[test]
    fn opening_a_call_inside_a_call_reuses_the_outer_view() {
        let reader = CallCountingReader::new(seeded());
        let outer = CallReader::open(&reader);

        let inner = CallReader::open(&outer);
        inner
            .read_element(&locator(), &expectation())
            .expect("read");

        assert_eq!(reader.views_opened(), 1);
        assert_eq!(reader.reads_through_views(), 1);
    }
}
