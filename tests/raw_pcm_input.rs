use songbird::input::{
    codecs::{get_codec_registry, get_probe},
    Input, RawAdapter,
};
use std::io::Cursor;
use symphonia_core::io::ReadOnlySource;

#[tokio::test]
async fn raw_pcm_input_is_playable() {
    let silence = vec![0_u8; 48_000 / 50 * 2 * std::mem::size_of::<f32>()];
    let source = ReadOnlySource::new(Cursor::new(silence));
    let input: Input = RawAdapter::new(source, 48_000, 2).into();

    let result = input
        .make_playable_async(get_codec_registry(), get_probe())
        .await;

    assert!(
        result.is_ok(),
        "raw PCM input should be playable: {:?}",
        result.err()
    );
}
