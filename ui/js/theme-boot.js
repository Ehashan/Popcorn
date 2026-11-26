// A classic script in <head>, so the saved theme applies before the first paint.
document.documentElement.dataset.theme =
  localStorage.getItem('popcorn.theme') === 'light' ? 'light' : 'dark';
