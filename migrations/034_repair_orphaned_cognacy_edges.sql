-- Repair cognacy edges that reference words no longer in the words table.
-- When a word is deleted, the FK cascade clears word_relations rows but the
-- cognacy json tree is independent storage and keeps the dangling edges. The
-- graph renderer then fails to find a node for the deleted word and errors out
-- when any other word in the cognacy is viewed.
--
-- This migration strips dangling edges from every cognacy, deletes cognacies
-- that end up empty, and clears `cognacy` pointers on words that no longer
-- appear in any of their cognacy's edges. Component splitting (when an edge
-- removal disconnects the graph into multiple cognacies) is left for the
-- application-level cleanup added in the same change to handle going forward;
-- on existing data, leaving disconnected components in a single cognacy still
-- renders correctly, just as multiple disconnected trees.

update cognacies c
set tree = jsonb_set(
    c.tree,
    '{edges}',
    coalesce((
        select jsonb_agg(elem)
        from jsonb_array_elements(c.tree->'edges') elem
        where exists (select 1 from words where id = (elem->>'antecedent')::uuid)
          and exists (select 1 from words where id = (elem->>'consequent')::uuid)
    ), '[]'::jsonb)
)
where exists (
    select 1
    from jsonb_array_elements(c.tree->'edges') elem
    where not exists (select 1 from words where id = (elem->>'antecedent')::uuid)
       or not exists (select 1 from words where id = (elem->>'consequent')::uuid)
);

-- Cascade on words.cognacy will set the orphaned words' cognacy to null.
delete from cognacies where jsonb_array_length(tree->'edges') = 0;

-- Words that survived but are no longer part of any edge in their cognacy:
-- their cognacy pointer is now stale, clear it.
update words w
set cognacy = null
where w.cognacy is not null
  and not exists (
      select 1
      from cognacies c, jsonb_array_elements(c.tree->'edges') elem
      where c.id = w.cognacy
        and ((elem->>'antecedent')::uuid = w.id or (elem->>'consequent')::uuid = w.id)
  );
