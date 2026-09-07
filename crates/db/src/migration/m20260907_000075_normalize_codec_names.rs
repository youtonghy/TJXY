use sea_orm::{ConnectionTrait, Statement};
use sea_orm_migration::prelude::{DbErr, DeriveMigrationName, MigrationTrait, SchemaManager};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rewrite_legacy_codec_names(manager).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Reversing the rewrite is ambiguous across container families: both
        // Matroska "a_ac3" and ISO-BMFF "ac-3" normalize to "ac3", and streams
        // probed after this migration cannot be attributed to either origin.
        // Codec values are re-derivable by re-probing, matching the precedent
        // set by m20260811_000057.
        Ok(())
    }
}

/// Codec values persisted by the previous probe normalization. Matroska ids
/// were lowercased with separators mapped to underscores; ISO-BMFF tags kept
/// their hyphenated form.
const LEGACY_CODEC_NAMES: &[(&str, &str)] = &[
    // Matroska video
    ("v_mpeg2", "mpeg2video"),
    ("v_mpeg1", "mpeg1video"),
    ("v_mpeg4_iso_asp", "mpeg4"),
    ("v_ms_vfw_fourcc", "vc1"),
    ("v_vp8", "vp8"),
    ("v_theora", "theora"),
    // Matroska audio
    ("a_ac3", "ac3"),
    ("a_eac3", "eac3"),
    ("a_dts", "dts"),
    ("a_truehd", "truehd"),
    ("a_flac", "flac"),
    ("a_alac", "alac"),
    ("a_mpeg_l3", "mp3"),
    ("a_aac_mpeg4_lc", "aac"),
    // Matroska subtitles
    ("s_ass", "ass"),
    ("s_ssa", "ssa"),
    ("s_hdmv_pgs", "pgssub"),
    ("s_vobsub", "dvdsub"),
    // ISO-BMFF audio and subtitles
    ("ac-3", "ac3"),
    ("ec-3", "eac3"),
    ("tx3g", "mov_text"),
];

async fn rewrite_legacy_codec_names(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    let assignments = LEGACY_CODEC_NAMES
        .iter()
        .map(|(legacy, normalized)| format!("WHEN '{legacy}' THEN '{normalized}'"))
        .collect::<Vec<_>>()
        .join(" ");
    let legacy_names = LEGACY_CODEC_NAMES
        .iter()
        .map(|(legacy, _)| format!("'{legacy}'"))
        .collect::<Vec<_>>()
        .join(", ");
    // A single-pass conditional rewrite is identical across SQLite, MySQL,
    // and PostgreSQL, so one raw statement keeps the migration portable.
    let sql = format!(
        "UPDATE media_streams SET codec = CASE codec {assignments} ELSE codec END \
         WHERE codec IN ({legacy_names})"
    );
    connection
        .execute(Statement::from_string(backend, sql))
        .await?;
    Ok(())
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_names_are_unique_so_the_rewrite_is_deterministic() {
        let mut legacy: Vec<&str> = LEGACY_CODEC_NAMES
            .iter()
            .map(|(legacy, _)| *legacy)
            .collect();
        legacy.sort_unstable();
        legacy.dedup();
        assert_eq!(
            legacy.len(),
            LEGACY_CODEC_NAMES.len(),
            "each legacy value must appear once so the CASE rewrite is deterministic"
        );
    }
}
