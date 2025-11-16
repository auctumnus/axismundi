get rid of the definition field on words

we add a definitions table that has a context label and definition, and which can be linked to by...

translations (new feature)

its like they had it on cws. you can post a text to be translated (in english, with an option for the original source language if not english)

then, if you have edit perms on a language, you can post a translation into your language

this includes a gloss field, which is where all the beautiful magic happens:

- you can annotate each part of the gloss with a link to a word
- we can parse the gloss field using a standardized form of leipzig glossing notation, and then assign words in the language to each of the words in the translation; probably needs some sort of nice, clickable ui, where we give the user a "search words" thing that prefills with the literal word they typed.
- each word you link to in the translation will then have the word appear as a quotation on the word itself, like how wiktionary does it
- by default, only one (random) quotation appears under each word, and you can click to go to a list of all translations in that language including that word
- probably least annoying to do this as a jsonb table or something; `translations` table has a field that links to the `translatable` its a translation of, then the text of the translation, gloss, ipa, notes, etc, and then we have a field like `gloss_data`:

[
    {
        "span": {
            "start": 0,
            "end: 3,
        },
        "bookmark": "dfkvjnJKDFBJKSD"
    },
    ...
]


the ui... it will be very difficult. i think it should work like:
1. we enter a "glossing mode", where
2. each word can be annotated with leipzig notation and also with links to words (and their definitions)
3. you can press "left" and "right" buttons on either your keyboard or the screen to go between words
4. you can select a definition, it defaults to the first one
4. we have a little "cut" button that puts a little slash between the words, allowing you to add different links. these show up in a little list, so that you can get things like

```
l'aurore bóreale
```

we style each "section" of a word with a repeating color palette, and when you tap on a linked part it lets you edit the link

sections also show in the editing menu to bring up the different parts of the link

then, when we display it, we can show it in bold in the quotations, and the translation itself can have an aligned leipzig gloss / ipa / etc, and each part can link to the word it is

if youre using noscript the editing functionality is just broken. i dont care

