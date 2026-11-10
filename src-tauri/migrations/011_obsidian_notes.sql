-- Obsidian notes linked to a workstream. `id` is "<vault>/<path>" (vault names can't
-- contain '/') and doubles as `workstream_items.item_id`. The notes themselves live in
-- the vault; a row exists only while the note is linked.
CREATE TABLE obsidian_notes (
    id    TEXT PRIMARY KEY,
    vault TEXT NOT NULL,
    path  TEXT NOT NULL
);
