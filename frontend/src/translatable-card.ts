document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll('#translatable-list .likes').forEach((likeButton) => {
        likeButton.addEventListener('click', async (event) => {
            event.preventDefault();
            const target = likeButton.getAttribute('data-target');
            const shouldLike = !likeButton.classList.contains('liked');
            const response = await fetch(`/api/translatable/${target}/${shouldLike ? 'like' : 'unlike'}`, {
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
