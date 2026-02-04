document.addEventListener("DOMContentLoaded", () => {
  const likeButtons = document.querySelectorAll(".like-button");

  for (const button of likeButtons) {
    const target = button.getAttribute("data-target");
    const likeEndpoint = `${target}/like`;
    const unlikeEndpoint = `${target}/unlike`;
    const likeCountSpan = button.querySelector(".like-count");

    let isLiked = button.classList.contains("liked");
    let likeCount = parseInt(likeCountSpan?.textContent || "0", 10);

    button.addEventListener("click", async (e) => {
      e.preventDefault();
      e.stopPropagation();

      let endpoint = likeEndpoint;

      if (isLiked) {
        endpoint = unlikeEndpoint;
      }

      try {
        const response = await fetch(endpoint, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
          },
          credentials: "include",
        });

        if (response.ok) {
          isLiked = !isLiked;
          button.classList.toggle("liked", isLiked);

          if (isLiked) {
            likeCount += 1;
          } else {
            likeCount -= 1;
          }

          if (likeCountSpan) {
            likeCountSpan.textContent = likeCount.toString();
          }
        } else {
          console.error("Failed to toggle like status:", response.statusText);
        }
      } catch (error) {
        console.error("Error toggling like status:", error);
      }
    });
  }
});
