import { initializeLikeButtons } from './like-button';

document.addEventListener("DOMContentLoaded", () => {
    initializeLikeButtons('#translation-list .likes', (target) => {
        const [translatableSlug, languageCode] = target.split('/');
        return `/api/translatable/${translatableSlug}/translations/${languageCode}`;
    });
});
