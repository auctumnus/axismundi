const relativeFormat = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

const staticFormat = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "long",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

const timeAgo = (date: Date) => {
  const now = new Date();

  const diff = date.getTime() - now.getTime(); // in milliseconds
  const diffSec = diff / 1000;
  const diffMin = diffSec / 60;
  const diffHour = diffMin / 60;
  const diffDay = diffHour / 24;
  const diffWeek = diffDay / 7;
  const diffMonth = diffDay / 30.44; // average days per month
  const diffYear = diffDay / 365.25;

  if (Math.abs(diffYear) >= 1)
    return relativeFormat.format(Math.round(diffYear), "year");
  if (Math.abs(diffMonth) >= 1)
    return relativeFormat.format(Math.round(diffMonth), "month");
  if (Math.abs(diffWeek) >= 1)
    return relativeFormat.format(Math.round(diffWeek), "week");
  if (Math.abs(diffDay) >= 1)
    return relativeFormat.format(Math.round(diffDay), "day");
  if (Math.abs(diffHour) >= 1)
    return relativeFormat.format(Math.round(diffHour), "hour");
  if (Math.abs(diffMin) >= 1)
    return relativeFormat.format(Math.round(diffMin), "minute");
  return relativeFormat.format(Math.round(diffSec), "second");
};

document.addEventListener("DOMContentLoaded", () => {
  const timeElements = document.querySelectorAll("time[datetime]");
  timeElements.forEach((timeElement) => {
    const datetime = timeElement.getAttribute("datetime");
    if (datetime) {
      const date = new Date(datetime);
      timeElement.textContent = timeAgo(date);
      timeElement.addEventListener("click", () => {
        timeElement.classList.toggle("static-time");
        if (timeElement.classList.contains("static-time")) {
          timeElement.textContent = staticFormat.format(date);
        } else {
          timeElement.textContent = timeAgo(date);
        }
      });
    }
  });
});
