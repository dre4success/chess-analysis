import { createRoot } from 'react-dom/client';
import Home from '../app/page';
import '../app/globals.css';
import '@fontsource/geist-mono/latin-400.css';
import '@fontsource/geist-mono/latin-500.css';
import '@fontsource/geist-mono/latin-600.css';
import '@fontsource/geist-mono/latin-700.css';
createRoot(document.getElementById('root')!).render(<Home />);
