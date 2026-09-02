type Result =
  | { status: 'rendered'; table_id: string; html: string }
  | { status: 'timed_out'; full_page_url: string }
  | { status: 'error'; message: string };

type Response = { results: Result[]; section_error: string | null };

const container = document.querySelector<HTMLElement>('#grammar-tables[data-url]');

function withIcon(link: HTMLAnchorElement, iconName: string, label: string) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('class', 'icon');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', `#icon-${iconName}`);
  svg.append(use);
  const text = document.createElement('span');
  text.textContent = label;
  link.append(svg, text);
}

function addTableHeader(section: HTMLElement, fullPageUrl: string) {
  const heading = section.querySelector(':scope > h2');
  if (!heading) return;

  const header = document.createElement('div');
  header.className = 'header-with-actions';
  const tableHeading = document.createElement('h3');
  tableHeading.textContent = heading.textContent;
  if (heading.id) tableHeading.id = heading.id;

  const actions = document.createElement('ul');
  const action = document.createElement('li');
  const link = document.createElement('a');
  link.className = 'with-icon';
  link.href = fullPageUrl;
  withIcon(link, 'list', 'view table');
  action.append(link);
  actions.append(action);
  header.append(tableHeading, actions);
  heading.replaceWith(header);
}

if (container) {
  const url = container.dataset.url!;
  const viewUrlBase = container.dataset.viewUrlBase!;
  fetch(url, { headers: { Accept: 'application/json' } })
    .then(async response => {
      if (!response.ok) throw new Error('grammar tables could not be loaded');
      return response.json() as Promise<Response>;
    })
    .then(response => {
      container.replaceChildren();
      if (response.section_error) {
        container.textContent = response.section_error;
        return;
      }
      for (const result of response.results) {
        if (result.status === 'rendered') {
          // The server canonical renderer escapes every dynamic value. This is
          // intentionally the only HTML insertion point for grammar tables.
          // Append the rendered section itself, matching the raw phonology
          // tables on the language page. A template also lets every matching
          // table remain a sibling in its server-provided order.
          const template = document.createElement('template');
          template.innerHTML = result.html;
          for (const table of template.content.querySelectorAll<HTMLElement>(
            'section.grammar-table-container',
          )) {
            addTableHeader(table, `${viewUrlBase}/${result.table_id}`);
          }
          container.append(template.content);
        } else if (result.status === 'timed_out') {
          const fallback = document.createElement('section');
          fallback.className = 'grammar-table-fallback';
          const message = document.createElement('p');
          message.textContent = 'this table took too long to render';
          const link = document.createElement('a');
          link.href = result.full_page_url;
          link.textContent = 'view as full page';
          fallback.append(message, link);
          container.append(fallback);
        } else {
          const fallback = document.createElement('section');
          fallback.className = 'grammar-table-fallback';
          const message = document.createElement('p');
          message.className = 'error';
          message.textContent = result.message;
          fallback.append(message);
          container.append(fallback);
        }
      }
      if (response.results.length === 0) {
        container.closest('#grammar-tables-section')?.remove();
      }
    })
    .catch(() => {
      container.textContent = 'grammar tables could not be loaded';
    });
}
