const estimate = async (word: string, soundChangeSetId: string, signal?: AbortSignal) => {
  const response = await fetch(
    `/api/sound-change-sets/${soundChangeSetId}/run`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ input_words: [word] }),
      signal,
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

const debounce = (func: Function, delay: number) => {
  let timeoutId: number | null = null;
  return (...args: any[]) => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = window.setTimeout(() => {
      func(...args);
      timeoutId = null;
    }, delay);
  };
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

    let controller: AbortController | null = null;

    const e = async (event: Event) => {
      const word = wordInput.value;
      if (controller) {
        controller.abort();
      }
      try {
        setRunning();
        const [estimatedIpa] = await Promise.all([
          (async () => {
            controller = new AbortController();
            const r = await estimate(word, soundChangeSetId, controller.signal);
            return r;
          })(),
          new Promise((resolve) => setTimeout(resolve, 500)), // ensure the saving state is visible for at least 500ms
        ]);
        ipaInput.value = estimatedIpa;
        setIdle();
      } catch (error) {
        if (error instanceof DOMException && error.name === "AbortError") {
          // Ignore abort errors
          return;
        }
        console.error("Error estimating IPA:", error);
        setErrored(
          error instanceof Error ? error.message : "Failed to estimate IPA",
        );
      }
    };

    estimateButton.addEventListener("click", (event) => {
      event.preventDefault(); e(event)
    });

    const wordEvent = (event: Event) => {
      debounce(() => e(event), 500)();
    }

    wordInput.addEventListener("input", wordEvent);

    ipaInput.addEventListener("input", () => {
      wordInput.removeEventListener("input", wordEvent);
      setIdle();
    });
  }
});
