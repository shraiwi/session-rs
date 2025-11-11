async function registerServiceWorker() {
  // Check if the browser supports service workers
  if ('serviceWorker' in navigator) {
    try {
      // Register the service-worker.js file
      // We add { type: 'module' } because your service worker uses 'import'
      const registration = await navigator.serviceWorker.register('./service-worker.js', {
        type: 'module' 
      });
      console.log('Service Worker registered successfully:', registration);
    } catch (error) {
      console.error('Service Worker registration failed:', error);
    }
  } else {
    console.warn('Service Workers are not supported in this browser.');
  }
}

// Run the registration function
registerServiceWorker();