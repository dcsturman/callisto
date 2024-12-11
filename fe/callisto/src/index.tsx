import React from 'react';
import ReactDOM from 'react-dom/client';
import { GoogleOAuthProvider } from '@react-oauth/google';
import './index.css';
import { GOOGLE_OAUTH_CLIENT_ID, App} from './App';
import reportWebVitals from './reportWebVitals';


const root = ReactDOM.createRoot(
  document.getElementById('root') as HTMLElement
);

document.body.style.overflow = "hidden";

root.render(
    <GoogleOAuthProvider clientId={GOOGLE_OAUTH_CLIENT_ID}>
  <React.StrictMode>
    <App />    
  </React.StrictMode>
  </GoogleOAuthProvider>
);

// If you want to start measuring performance in your app, pass a function
// to log results (for example: reportWebVitals(console.log))
// or send to an analytics endpoint. Learn more: https://bit.ly/CRA-vitals
reportWebVitals();
