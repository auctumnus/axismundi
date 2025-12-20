document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll('#word-list .likes').forEach((likeButton) => {
        likeButton.addEventListener('click', async (event) => {
            event.preventDefault();
            event.stopPropagation();
            const target = likeButton.getAttribute('data-target');
            const [languageCode, slug, lemma] = target.split('/');
            const shouldLike = !likeButton.classList.contains('liked');
            const response = await fetch(`/api/languages/${languageCode}/words/${slug}/${lemma}/${shouldLike ? 'like' : 'unlike'}`, {
                method: 'POST',
            });
            if (response.ok) {
                const data = await response.json();
                const likeCountSpan = likeButton.querySelector('span.like-count')!;
                likeCountSpan.textContent = data.like_count;
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
        });
    })
})
