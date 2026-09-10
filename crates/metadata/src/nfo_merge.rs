use super::NfoDocument;

impl NfoDocument {
    /// Combines missing fields only after checking every nonempty movie-level field.
    /// Per-part runtimes are omitted for multipart movies; probe results own media timing.
    ///
    /// # Errors
    /// Returns stable field names when the candidates describe conflicting metadata.
    #[allow(clippy::float_cmp)] // Parsed ratings must agree exactly before an automatic merge.
    pub fn merge_consistent(
        mut self,
        other: Self,
        multipart: bool,
    ) -> Result<Self, Vec<&'static str>> {
        let mut conflicts = Vec::new();
        if self.kind != other.kind {
            conflicts.push("kind");
        }
        macro_rules! merge_optional {
            ($($field:ident),+ $(,)?) => { $(
                match (&self.$field, &other.$field) {
                    (Some(left), Some(right)) if left != right => conflicts.push(stringify!($field)),
                    (None, Some(_)) => self.$field = other.$field,
                    _ => {},
                }
            )+ };
        }
        merge_optional!(
            title,
            original_title,
            production_year,
            overview,
            outline,
            community_rating,
            vote_count,
            premiere_date,
            end_date,
            release_status,
            official_rating,
            original_language
        );
        if multipart {
            self.runtime_ticks = None;
        } else {
            merge_optional!(runtime_ticks);
        }
        for (provider, id) in other.provider_ids {
            if self
                .provider_ids
                .get(&provider)
                .is_some_and(|current| current != &id)
            {
                conflicts.push("provider_ids");
            } else {
                self.provider_ids.insert(provider, id);
            }
        }
        macro_rules! merge_list {
            ($($field:ident),+ $(,)?) => { $(
                if self.$field.is_empty() { self.$field = other.$field; }
                else if !other.$field.is_empty() && (self.$field.len() != other.$field.len()
                    || !self.$field.iter().all(|value| other.$field.contains(value))) {
                    conflicts.push(stringify!($field));
                }
            )+ };
        }
        merge_list!(genres, studios, people);
        conflicts.sort_unstable();
        conflicts.dedup();
        if conflicts.is_empty() {
            Ok(self)
        } else {
            Err(conflicts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nfo(fields: &str) -> NfoDocument {
        NfoDocument::parse(
            format!("<movie><title>Arrival</title>{fields}</movie>").as_bytes(),
            "fixture",
        )
        .unwrap()
    }

    #[test]
    fn equal_candidates_merge_missing_fields_and_ignore_list_order() {
        let left = nfo("<genre>Drama</genre><genre>SciFi</genre><year>2016</year>");
        let right = nfo("<genre>SciFi</genre><genre>Drama</genre><plot>Contact</plot>");
        let merged = left.merge_consistent(right, false).unwrap();
        assert_eq!(merged.production_year(), Some(2016));
        assert_eq!(merged.overview(), Some("Contact"));
    }

    #[test]
    fn conflicting_identifiers_and_descriptions_require_selection() {
        let left = nfo("<uniqueid type=\"tmdb\">1</uniqueid><plot>First</plot>");
        let right = nfo("<uniqueid type=\"tmdb\">2</uniqueid><plot>Second</plot>");
        assert_eq!(
            left.merge_consistent(right, false).unwrap_err(),
            vec!["overview", "provider_ids"]
        );
    }

    #[test]
    fn multipart_runtime_differences_do_not_create_two_movie_sources() {
        let left = nfo("<runtime>50</runtime>");
        let right = nfo("<runtime>60</runtime>");
        assert!(left.clone().merge_consistent(right.clone(), false).is_err());
        assert_eq!(
            left.merge_consistent(right, true).unwrap().runtime_ticks(),
            None
        );
    }
}
