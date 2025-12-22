import { initializeLikeButtons } from './like-button';

document.addEventListener("DOMContentLoaded", () => {
    initializeLikeButtons('#word-list .likes', (target) => {
        const [languageCode, slug, lemma] = target.split('/');
        return `/api/languages/${languageCode}/words/${slug}/${lemma}`;
    });
});
