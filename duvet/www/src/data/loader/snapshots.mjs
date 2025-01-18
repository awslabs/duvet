import loadS2nQuic from '../../../../../integration/snapshots/s2n-quic_json.snap';
import loadS2nTls from '../../../../../integration/snapshots/s2n-tls_json.snap';
import loadEsdk from '../../../../../integration/snapshots/aws-encryption-sdk-dafny_json.snap';

export default async () => {
  const s2n_quic = await loadS2nQuic;
  const s2n_tls = await loadS2nTls;
  const esdk = await loadEsdk;

  s2n_quic.title = 's2n-quic';
  s2n_tls.title = 's2n-tls';
  esdk.title = 'AWS Encryption SDK - Dafny';

  return [s2n_quic, s2n_tls, esdk];
};
