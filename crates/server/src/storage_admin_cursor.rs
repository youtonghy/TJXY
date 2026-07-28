use std::collections::{HashMap, VecDeque};

use tjxy_storage::PageToken;
use uuid::Uuid;

const MAX_DIRECTORY_PAGE_CURSORS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryPageCursorError {
    UnknownOrMismatched,
}

struct DirectoryPageCursor<Context> {
    context: Context,
    provider_token: PageToken,
}

pub(crate) struct DirectoryPageCursorRegistry<Context> {
    entries: HashMap<Uuid, DirectoryPageCursor<Context>>,
    insertion_order: VecDeque<Uuid>,
}

impl<Context> Default for DirectoryPageCursorRegistry<Context> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
        }
    }
}

impl<Context: Eq> DirectoryPageCursorRegistry<Context> {
    pub(crate) fn resolve(
        &self,
        cursor: Option<Uuid>,
        context: &Context,
    ) -> Result<Option<PageToken>, DirectoryPageCursorError> {
        let Some(cursor) = cursor else {
            return Ok(None);
        };
        let entry = self
            .entries
            .get(&cursor)
            .filter(|entry| &entry.context == context)
            .ok_or(DirectoryPageCursorError::UnknownOrMismatched)?;
        Ok(Some(entry.provider_token.clone()))
    }

    pub(crate) fn register(
        &mut self,
        context: Context,
        provider_token: Option<PageToken>,
    ) -> Option<Uuid> {
        let provider_token = provider_token?;
        if let Some((cursor, _)) = self
            .entries
            .iter()
            .find(|(_, entry)| entry.context == context && entry.provider_token == provider_token)
        {
            return Some(*cursor);
        }
        if self.entries.len() == MAX_DIRECTORY_PAGE_CURSORS
            && let Some(oldest) = self.insertion_order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        let cursor = loop {
            let candidate = Uuid::new_v4();
            if !self.entries.contains_key(&candidate) {
                break candidate;
            }
        };
        self.entries.insert(
            cursor,
            DirectoryPageCursor {
                context,
                provider_token,
            },
        );
        self.insertion_order.push_back(cursor);
        Some(cursor)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use tjxy_storage::PageToken;
    use uuid::Uuid;

    use super::{DirectoryPageCursorError, DirectoryPageCursorRegistry};

    #[test]
    fn cursor_registry_is_replayable_context_bound_and_reuses_output() {
        let mut registry = DirectoryPageCursorRegistry::default();
        let context = ("MyDrive".to_owned(), "root".to_owned());
        let provider = PageToken::new("google-provider-page-2").unwrap();

        let cursor = registry
            .register(context.clone(), Some(provider.clone()))
            .unwrap();
        assert_eq!(
            registry.register(context.clone(), Some(provider.clone())),
            Some(cursor)
        );
        assert_eq!(
            registry.resolve(Some(cursor), &context).unwrap(),
            Some(provider)
        );
        assert_eq!(
            registry.resolve(Some(cursor), &("MyDrive".to_owned(), "other".to_owned())),
            Err(DirectoryPageCursorError::UnknownOrMismatched),
        );
        assert_eq!(
            registry.resolve(Some(Uuid::new_v4()), &context),
            Err(DirectoryPageCursorError::UnknownOrMismatched),
        );
        assert_eq!(registry.resolve(None, &context).unwrap(), None);
        assert_eq!(registry.register(context, None), None);
    }

    #[test]
    fn cursor_registry_evicts_the_oldest_entry_at_its_fixed_bound() {
        let mut registry = DirectoryPageCursorRegistry::default();
        let first_context = 0_u16;
        let first = registry
            .register(
                first_context,
                Some(PageToken::new("provider-page-0").unwrap()),
            )
            .unwrap();
        for value in 1_u16..=256 {
            let _ = registry.register(
                value,
                Some(PageToken::new(format!("provider-page-{value}")).unwrap()),
            );
        }
        assert_eq!(registry.len(), 256);
        assert_eq!(
            registry.resolve(Some(first), &first_context),
            Err(DirectoryPageCursorError::UnknownOrMismatched),
        );
    }
}
