import React from 'react';
import ReactDOM from 'react-dom/client';
import { GoogleOAuthProvider } from '@react-oauth/google';
import './index.css';
import App from './App';
import reportWebVitals from './reportWebVitals';


const root = ReactDOM.createRoot(
  document.getElementById('root') as HTMLElement
);

document.body.style.overflow = "hidden";

// Awkward adn brittle to hardcode the clientID in here but its not a secret.
root.render(
  <GoogleOAuthProvider clientId="402344016908-a6k9ekcrnmcaki9bl32io9cjp2jtanv5.apps.googleusercontent.com">
  <React.StrictMode>
    <App />    
  </React.StrictMode>
  </GoogleOAuthProvider>
);

// If you want to start measuring performance in your app, pass a function
// to log results (for example: reportWebVitals(console.log))
// or send to an analytics endpoint. Learn more: https://bit.ly/CRA-vitals
reportWebVitals();
