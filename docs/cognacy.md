cognacies are etymology graphs; in the DB, they are a json + schema number which describes the graph.
we may not need to change the schema version but better safe than sorry!

v1 works like:
- etymology is a graph
- nodes are words, edges are word_relations, they also have an edge weight of the kind of relation it is

when we give the cognacy back to the api, it looks like
{
    schema_version: 1
    edges: {
        [
            {
                kind: "borrowed",
                antecedent: "really-long-uid",
                consequent: "another-one",
            }
        ]
    },
    words: {
        "really-long-uuid": {
            word: "aaa",
            ipa: "aáa",
            definition: "...",
            ... rest of the Word from the word model etc
        },
        "another-one": { ... } 
    },
}

the consequent is where the arrow points to

when we add an edge, we have to merge the graphs of each and check for cycles
(should never have any)

when we remove an edge, we have to check whether we have 2 separate graphs

we never update them just remove->add

see_also is not a valid kind of cognacy relation but it is a word relation, so you can have
2 words see_also each other; normally this isnt allowed (because etymology is a DAG)