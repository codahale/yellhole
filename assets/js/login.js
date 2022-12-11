document.addEventListener('DOMContentLoaded', (_) => {
  if (!window.PublicKeyCredential || !PublicKeyCredential.isConditionalMediationAvailable) {
    return;
  }

  Promise.all([
    PublicKeyCredential.isConditionalMediationAvailable(),
    PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()])
    .then((values) => {
      if (values.every(x => x === true)) {
        document.getElementById('login').disabled = false;
      }
    });
});

async function login() {
  const startResp = await fetch('/login/start', {
    method: 'POST'
  }).catch((error) => { console.error(error) });

  if (!startResp.ok) {
    window.alert('Error starting passkey authentication.');
    return;
  }

  const startJson = await startResp.json()
    .catch((error) => { console.error(error) });


  const authOptions = {
    publicKey: {
      challenge: Uint8Array.from(atob(startJson.challengeBase64), c => c.charCodeAt(0)),
      rpId: startJson.rpId,
      allowCredentials: startJson.passkeyIdsBase64.map(id => {
        return {
          type: 'public-key',
          id: Uint8Array.from(atob(id), c => c.charCodeAt(0)),
        };
      }),
    },
  };

  const assertion = await navigator.credentials.get(authOptions).catch(error => console.log(error));
  if (!assertion) {
    window.alert('Error authentication with passkey.')
    return;
  }

  const auth = {
    rawIdBase64: btoa(String.fromCharCode(...new Uint8Array(assertion.rawId))),
    clientDataJSONBase64: btoa(new TextDecoder().decode(assertion.response.clientDataJSON)),
    authenticatorDataBase64: btoa(String.fromCharCode(...new Uint8Array(assertion.response.authenticatorData))),
    signatureBase64: btoa(String.fromCharCode(...new Uint8Array(assertion.response.signature))),
  };

  const finishResp = await fetch('/login/finish', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(auth),
  });

  if (finishResp.ok) {
    window.location.href = '/admin/new';
  } else {
    window.alert('Error finishing passkey authentication.');
  }
}
