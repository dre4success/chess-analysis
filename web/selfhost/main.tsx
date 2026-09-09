import { createRoot } from 'react-dom/client';
import Home from '../app/page';
import '../app/globals.css';
import '@fontsource/geist/latin-400.css';
import '@fontsource/geist/latin-500.css';
import '@fontsource/geist/latin-600.css';
import '@fontsource/geist/latin-700.css';
import '@fontsource/geist-mono/latin-400.css';
createRoot(document.getElementById('root')!).render(<Home />);
