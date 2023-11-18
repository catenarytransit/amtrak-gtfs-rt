// decrypting Amtrak's real-time train location geoJSON feed
// based on https://github.com/Vivalize/Amtrak-Train-Stats
const fetch = require('node-fetch');
const CryptoJS = require('crypto-js');

// this is the xhr call done by https://www.amtrak.com/track-your-train.html containing encrypted train location data
const dataUrl = 'https://maps.amtrak.com/services/MapDataService/trains/getTrainsData';

// these constants are pulled from RoutesList.v.json, which is an object with keys 'arr', 's', and 'v'
const sValue = '9a3686ac'; // found at s[8]
const iValue = 'c6eb2f7f5c4740c1a2f708fefd947d39'; // found at v[32]
const publicKey = '69af143c-e8cf-47f8-bf09-fc1f61e5cc33'; // found at arr[179]

// the encrypted data returned by dataURL contains two hashes, the second one is {masterSegment} characters long
const masterSegment = 88;

// Decrypt with CryptoJS
function decrypt(content, key) {
	return CryptoJS.AES.decrypt(
		CryptoJS.lib.CipherParams.create({
      ciphertext: CryptoJS.enc.Base64.parse(content)
    }),
		CryptoJS.PBKDF2(key, CryptoJS.enc.Hex.parse(sValue), {
      keySize: 4, iterations: 1e3
    }),
		{
      iv: CryptoJS.enc.Hex.parse(iValue)
    },
	).toString(CryptoJS.enc.Utf8)
};

// Decrypt the data and clean it up
function decryptData(encryptedData) {
  const contentHashLength = encryptedData.length - masterSegment;

  // get the two parts of the
	const contentHash = encryptedData.slice(0, contentHashLength);
  const privateKeyHash = encryptedData.slice(contentHashLength);

	const privateKey = decrypt(privateKeyHash, publicKey).split('|')[0]
	const { TrainsDataResponse: geoJSON } = JSON.parse(decrypt(contentHash, privateKey));

  return geoJSON;
};

(async () => {
  const encryptedData = await fetch(dataUrl).then(d => d.text());
  const geoJSON = decryptData(encryptedData);
  console.log(geoJSON.features[0]);
})();

// what, lol, looks like all of this is unnecessary because it's all in a public carto table
// https://amtk.carto.com/api/v2/sql?q=select * from active_trains&format=geojson&api_key={api_key}