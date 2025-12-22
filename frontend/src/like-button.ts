/**
 * Initialize like buttons for a specific resource type
 * @param selector - CSS selector to find like buttons (e.g., '#language-list .likes')
 * @param getApiPath - Function that takes a target string and returns the API path prefix
 *                     (e.g., (target) => `/api/languages/${target}`)
 */
export function initializeLikeButtons(
    selector: string,
    getApiPath: (target: string) => string
): void {
    document.querySelectorAll(selector).forEach((likeButton) => {
        likeButton.addEventListener('click', async (event) => {
            event.preventDefault();
            event.stopPropagation();

            const target = likeButton.getAttribute('data-target');
            if (!target) {
                console.error('Like button missing data-target attribute');
                return;
            }

            const shouldLike = !likeButton.classList.contains('liked');
            const apiPath = getApiPath(target);
            const endpoint = `${apiPath}/${shouldLike ? 'like' : 'unlike'}`;

            try {
                const response = await fetch(endpoint, {
                    method: 'POST',
                });

                if (response.ok) {
                    const data = await response.json();
                    const likeCountSpan = likeButton.querySelector('span.like-count');

                    if (likeCountSpan) {
                        likeCountSpan.textContent = data.like_count;
                    }

                    if (data.liked) {
                        likeButton.classList.add('liked');
                        likeButton.classList.add('animating');
                        setTimeout(() => {
                            likeButton.classList.remove('animating');
                        }, 400);
                    } else {
                        likeButton.classList.remove('liked');
                        likeButton.classList.remove('animating');
                    }
                } else {
                    console.error('Failed to toggle like status');
                }
            } catch (error) {
                console.error('Error toggling like status:', error);
            }
        });
    });
}
