import { initializeLikeButtons } from './like-button';

document.addEventListener("DOMContentLoaded", () => {
    // Handle likes on full card view page
    initializeLikeButtons('.card .likes', (target) => `/api/translatable/${target}`);

    // Handle likes on translatable list
    initializeLikeButtons('#translatable-list .likes', (target) => `/api/translatable/${target}`);
});
