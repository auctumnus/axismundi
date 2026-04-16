const estimate = async (word: string, soundChangeSetId: string) => {
  const response = await fetch(
    `/api/sound-change-sets/${soundChangeSetId}/run`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ input_words: [word] }),
    },
  );
  if (response.ok) {
    const data = await response.json();
    return data.outputWords[0];
  }
  const errorText = await response.text();
  throw new Error(errorText || "Failed to estimate IPA");
};

const getOrCreateFieldErrors = (section: HTMLElement): HTMLUListElement => {
  let ul = section.querySelector<HTMLUListElement>("ul.field-errors");
  if (!ul) {
    ul = document.createElement("ul");
    ul.className = "field-errors";
    const ipaEstimator = section.querySelector(".ipa-estimator");
    if (ipaEstimator) {
      section.insertBefore(ul, ipaEstimator);
    } else {
      section.appendChild(ul);
    }
  }
  return ul;
};

const clearFieldErrors = (section: HTMLElement) => {
  const ul = section.querySelector("ul.field-errors");
  if (ul) ul.remove();
};

document.addEventListener("DOMContentLoaded", () => {
  const wordInput = document.getElementById("word") as HTMLInputElement | null;
  const ipaInput = document.getElementById("ipa") as HTMLInputElement | null;

  if (!wordInput || !ipaInput) return;

  const estimateButton = document.getElementById(
    "estimate-ipa",
  ) as HTMLButtonElement | null;
  if (estimateButton) {
    const ipaSection = ipaInput.closest("section") as HTMLElement | null;

    const setRunning = () => {
      estimateButton.disabled = true;
      estimateButton.textContent = "Estimating...";
      if (ipaSection) clearFieldErrors(ipaSection);
    };

    const setIdle = () => {
      estimateButton.disabled = false;
      estimateButton.textContent = "Estimate IPA";
    };

    const setErrored = (message: string) => {
      estimateButton.disabled = false;
      estimateButton.textContent = "Estimate IPA";
      if (ipaSection) {
        const ul = getOrCreateFieldErrors(ipaSection);
        ul.innerHTML = "";
        const li = document.createElement("li");
        li.textContent = message;
        ul.appendChild(li);
      }
    };

    const estimatorHint = document.getElementById("ipa-estimator-hint");
    if (estimatorHint) {
      estimatorHint.classList.remove("hidden");
    }

    const soundChangeSetId = estimateButton.getAttribute(
      "data-sound-change-set",
    );
    if (!soundChangeSetId) return;
    estimateButton.addEventListener("click", async (event) => {
      event.preventDefault();
      const word = wordInput.value;
      try {
        setRunning();
        const [estimatedIpa] = await Promise.all([
          (async () => {
            const r = await estimate(word, soundChangeSetId);
            return r;
          })(),
          new Promise((resolve) => setTimeout(resolve, 500)), // ensure the saving state is visible for at least 500ms
        ]);
        ipaInput.value = estimatedIpa;
        setIdle();
      } catch (error) {
        console.error("Error estimating IPA:", error);
        setErrored(
          error instanceof Error ? error.message : "Failed to estimate IPA",
        );
      }
    });
  }
});
