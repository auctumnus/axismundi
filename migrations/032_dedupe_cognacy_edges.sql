-- Deduplicate cognacy edges that were doubled by a bug in word_relations::merge
-- when adding a relation between two words already in the same cognacy graph.
-- Duplicates are byte-identical (Copy of CognacyEdgeV1 with the same id), so
-- jsonb_agg(DISTINCT ...) collapses them correctly.

with deduped as (
    select
        c.id,
        jsonb_agg(distinct elem) as unique_edges
    from cognacies c,
         jsonb_array_elements(c.tree->'edges') as elem
    group by c.id
)
update cognacies c
set tree = jsonb_set(c.tree, '{edges}', d.unique_edges)
from deduped d
where c.id = d.id
  and jsonb_array_length(c.tree->'edges') <> jsonb_array_length(d.unique_edges);
