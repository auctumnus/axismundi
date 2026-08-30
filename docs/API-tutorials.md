# API tutorials

These walkthroughs show complete, scriptable workflows with the current API.
They assume a local shell with `curl` and `jq`.

- Full endpoint and field reference: [API reference](API.md)
- All paths below are relative to `$BASE_URL/api`.
- Error responses are plain text. While developing, omit `-f` from `curl` if
  you want to inspect an error body.

## Tutorial: add a word and derive it into a daughter language

This tutorial creates *pater* in a parent language, applies a sound-change set
to produce *peter* in a daughter language, creates the daughter word, and
records the etymological link.

The result has two distinct, useful pieces of data:

1. A stored word in each language.
2. A directed `descendant` relation from the parent word to the daughter word.

The relation causes the two words to share a cognacy graph. It does not run
sound changes or create the daughter word for you; those are deliberate,
separate API operations.

### Before you start

You need:

- A verified account and its API token.
- Editor-or-higher access to both languages.
- A word class with abbreviation `n` in both languages. You can use another
  abbreviation; use the same value in the word-create bodies below.
- The parent and daughter language codes. This example uses `proto` and
  `daughter`.

Set up the shell variables:

```bash
BASE_URL='https://axismundi.example'
TOKEN='your-api-token'
PARENT_CODE='proto'
DAUGHTER_CODE='daughter'
AUTH_HEADER="Authorization: Bearer $TOKEN"
```

### Create words with `axm`

The included resource-oriented client exposes the word-create step directly:

```bash
export AXISMUNDI_API_URL="$BASE_URL/api"
export AXISMUNDI_API_TOKEN="$TOKEN"

parent_word=$(cargo run --quiet --bin axm -- word new \
  --in "$PARENT_CODE" --word pater --def father --class n --ipa pa.ter \
  --notes 'Illustrative parent-language form.')
PARENT_SLUG=$(jq -r '.slug' <<<"$parent_word")
PARENT_LEMMA=$(jq -r '.lemma' <<<"$parent_word")
```

The same command creates the daughter word after you have run the sound
changes:

```bash
daughter_word=$(cargo run --quiet --bin axm -- word new \
  --in "$DAUGHTER_CODE" --word "$DAUGHTER_FORM" --def father --class n)
DAUGHTER_SLUG=$(jq -r '.slug' <<<"$daughter_word")
DAUGHTER_LEMMA=$(jq -r '.lemma' <<<"$daughter_word")
```

Repeat `--def` to create several ordered definitions or `--category` to add
several categories. The client writes successful JSON to stdout, so command
substitution and `jq` work as shown; errors and non-success HTTP statuses go
to stderr and cause a non-zero exit.

To create a missing noun word class, send this once for each language:

```bash
curl -fsS -X POST "$BASE_URL/api/languages/$PARENT_CODE/word-classes" \
  -H "$AUTH_HEADER" \
  -H 'Content-Type: application/json' \
  --data '{"name":"noun","abbreviation":"n"}'
```

### 1. Create the parent word

A word must have a `word` and a `word_class`. Definitions are separate
resources, but a word-create request can create them in the same transaction.

```bash
parent_word=$(
  curl -fsS -X POST "$BASE_URL/api/languages/$PARENT_CODE/words" \
    -H "$AUTH_HEADER" \
    -H 'Content-Type: application/json' \
    --data '{
      "word": "pater",
      "word_class": "n",
      "ipa": "pa.ter",
      "notes": "Illustrative parent-language form.",
      "definitions": [
        { "definition": "father", "context": "kinship term" }
      ]
    }'
)

printf '%s\n' "$parent_word" | jq .
PARENT_SLUG=$(jq -r '.slug' <<<"$parent_word")
PARENT_LEMMA=$(jq -r '.lemma' <<<"$parent_word")
```

Never construct a word URL from the spelling alone. Save the returned
`slug` and `lemma`: a slug can have multiple lemmas, and the server
normalizes a word before generating its slug.

### 2. Store a sound-change set on the daughter language

The example rule changes every `a` to `e`, so it maps `pater` to
`peter`. Replace it with the real Lexurgy rules for the daughter language.

```bash
sound_change_set=$(
  curl -fsS -X POST "$BASE_URL/api/languages/$DAUGHTER_CODE/sound-change-sets" \
    -H "$AUTH_HEADER" \
    -H 'Content-Type: application/json' \
    --data '{
      "name": "Parent to daughter vowel shift",
      "description": "Illustrative a-to-e change.",
      "changes": "rule:\n  a => e"
    }'
)

SOUND_CHANGE_SET_ID=$(jq -r '.id' <<<"$sound_change_set")
```

Sound-change-set creation requires editor access to the language. Once
created, a stored set can be run by ID.

### 3. Run the stored set

The stored-set run endpoint takes snake_case `input_words`; its Lexurgy
response uses camelCase fields such as `outputWords`.

```bash
derivation=$(
  curl -fsS -X POST "$BASE_URL/api/sound-change-sets/$SOUND_CHANGE_SET_ID/run" \
    -H 'Content-Type: application/json' \
    --data '{"input_words":["pater"]}'
)

printf '%s\n' "$derivation" | jq .
DAUGHTER_FORM=$(jq -r '.outputWords[0]' <<<"$derivation")
test "$DAUGHTER_FORM" != 'null'
```

For debugging a rule set without storing it, use
`POST /api/sound-change-sets/run` instead. That endpoint accepts
`changes` and `inputWords` (camelCase), plus optional `traceWords`,
`startAt`, `stopBefore`, and `allowPolling`.

### 4. Create the daughter word

Use the output from the sound-change service as the daughter word's spelling.
The API does not automatically copy definitions, IPA, categories, or notes;
choose the appropriate data for the derived entry.

```bash
daughter_payload=$(
  jq -n --arg word "$DAUGHTER_FORM" '{
    word: $word,
    word_class: "n",
    ipa: "pe.ter",
    notes: "Regular descendant of parent pater.",
    definitions: [
      { definition: "father", context: "kinship term" }
    ]
  }'
)

daughter_word=$(
  curl -fsS -X POST "$BASE_URL/api/languages/$DAUGHTER_CODE/words" \
    -H "$AUTH_HEADER" \
    -H 'Content-Type: application/json' \
    --data "$daughter_payload"
)

DAUGHTER_SLUG=$(jq -r '.slug' <<<"$daughter_word")
DAUGHTER_LEMMA=$(jq -r '.lemma' <<<"$daughter_word")
```

### 5. Link the etymology

Create the relation from the older/parent form to the later/daughter form.
Use `descendant` for language-to-language descent. `derived` is for a
derivational relationship; it is not the usual label for a regular daughter
form.

```bash
relation_payload=$(
  jq -n \
    --arg language "$DAUGHTER_CODE" \
    --arg slug "$DAUGHTER_SLUG" \
    --argjson lemma "$DAUGHTER_LEMMA" \
    '{
      kind: "descendant",
      language: $language,
      slug: $slug,
      lemma: $lemma
    }'
)

curl -fsS -X POST \
  "$BASE_URL/api/languages/$PARENT_CODE/words/$PARENT_SLUG/$PARENT_LEMMA/relations" \
  -H "$AUTH_HEADER" \
  -H 'Content-Type: application/json' \
  --data "$relation_payload" | jq .
```

The caller must be allowed to edit the relevant words. The server rejects a
relation that would create a cycle in the cognacy graph.

### 6. Verify the result

Fetch the parent word's etymology:

```bash
curl -fsS \
  "$BASE_URL/api/languages/$PARENT_CODE/words/$PARENT_SLUG/$PARENT_LEMMA/etymology" \
  -H "$AUTH_HEADER" | jq .
```

For a list-oriented view, use:

```bash
curl -fsS \
  "$BASE_URL/api/languages/$PARENT_CODE/words/$PARENT_SLUG/$PARENT_LEMMA/relations?direction=antecedent" \
  -H "$AUTH_HEADER" | jq .
```

The parent word is the antecedent, and its daughter is the consequent. The
same graph is also available as SVG at the corresponding `.svg` endpoint.

## Optional: model the language relationship too

A word relation works without language-family records. To make the language
tree explicit, first create a family (or use one where you are an editor),
then attach the parent language as a root member and the daughter as its
child:

```bash
family=$(
  curl -fsS -X POST "$BASE_URL/api/language-families" \
    -H "$AUTH_HEADER" \
    -H 'Content-Type: application/json' \
    --data '{"code":"example-family","name":"Example family","description":"Tutorial family"}'
)
FAMILY_CODE=$(jq -r '.code' <<<"$family")

parent_member_payload=$(jq -n --arg language "$PARENT_CODE" \
  '{ language_code: $language, relation_type: "descendant" }')

curl -fsS -X POST "$BASE_URL/api/language-family/$FAMILY_CODE/members" \
  -H "$AUTH_HEADER" \
  -H 'Content-Type: application/json' \
  --data "$parent_member_payload"

daughter_member_payload=$(jq -n --arg language "$DAUGHTER_CODE" \
  '{ language_code: $language, relation_type: "descendant" }')

curl -fsS -X POST \
  "$BASE_URL/api/language-family/$FAMILY_CODE/members/by-code/$PARENT_CODE/children" \
  -H "$AUTH_HEADER" \
  -H 'Content-Type: application/json' \
  --data "$daughter_member_payload"
```

The family-member API intentionally uses the singular `/language-family`
path. Its `by-code` routes contain two `{code}` parameters in the router;
the first is the family code and the second is the language code, as shown in
the example.

## Common variations

- To add another sense after creating a word, `POST` a
  `{ "definition": "…", "context": "…" }` body to
  `/languages/{code}/words/{slug}/{lemma}/definitions`.
- To model a borrowing rather than inheritance, use `kind: "borrowed"`
  when creating the word relation.
- To inspect a graph visually, request
  `/languages/{code}/words/{slug}/{lemma}/etymology.svg`.
- To derive several forms at once, send each source spelling in the
  `input_words` array. The response's `outputWords` is in the same order.
