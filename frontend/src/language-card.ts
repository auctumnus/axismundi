import { initializeLikeButtons } from './like-button';

document.addEventListener("DOMContentLoaded", () => {
    initializeLikeButtons('#language-list .likes', (target) => `/api/languages/${target}`);
});