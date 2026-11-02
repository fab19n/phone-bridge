//! Test 0F: do rapid-fire control events from the page reach the control socket in order (REVIEW B6)?
//! Each variant feeds sequence numbers through a single writer task into a loopback TCP socket that stands
//! in for scrcpy's control socket; a reader on the other end counts inversions. Variants: one async
//! command per event (runs on the async runtime, so calls can overtake each other), one sync command per
//! event (runs on the main thread), and batches of events per call.

use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use tauri::async_runtime::JoinHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

#[derive(Default)]
pub struct OrderingState(Mutex<Option<Run>>);

pub struct Run {
    tx: UnboundedSender<u32>,
    writer: JoinHandle<std::io::Result<()>>,
    reader: JoinHandle<std::io::Result<Vec<u32>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderingStats {
    received: usize,
    /// Events that arrived after an event with a higher sequence number.
    inversions: usize,
    /// The largest distance an event moved back from its place.
    max_displacement: u32,
    missing: usize,
    duplicates: usize,
    /// The first few inversions as (previous, current) pairs.
    examples: Vec<(u32, u32)>,
}

#[tauri::command]
pub async fn spike_ordering_start(state: State<'_, OrderingState>) -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let (client, server) = tokio::try_join!(TcpStream::connect(addr), async {
        listener.accept().await.map(|(stream, _)| stream)
    })
    .map_err(|e| e.to_string())?;
    client.set_nodelay(true).map_err(|e| e.to_string())?;

    let (tx, mut rx) = unbounded_channel::<u32>();
    // The single writer: the only code that touches the socket, in queue order.
    let writer = tauri::async_runtime::spawn(async move {
        let mut client = client;
        while let Some(seq) = rx.recv().await {
            client.write_all(&seq.to_be_bytes()).await?;
        }
        client.shutdown().await
    });
    let reader = tauri::async_runtime::spawn(async move {
        let mut server = server;
        let mut bytes = Vec::new();
        server.read_to_end(&mut bytes).await?;
        Ok(bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_be_bytes(*c))
            .collect())
    });

    let previous = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .replace(Run { tx, writer, reader });
    if let Some(old) = previous {
        old.writer.abort();
        old.reader.abort();
    }
    Ok(())
}

fn enqueue(state: &OrderingState, seqs: &[u32]) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let run = guard.as_ref().ok_or("call spike_ordering_start first")?;
    for &seq in seqs {
        run.tx.send(seq).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn spike_ordering_send_async(
    seq: u32,
    state: State<'_, OrderingState>,
) -> Result<(), String> {
    enqueue(&state, &[seq])
}

#[tauri::command]
pub fn spike_ordering_send_sync(seq: u32, state: State<'_, OrderingState>) -> Result<(), String> {
    enqueue(&state, &[seq])
}

#[tauri::command]
pub fn spike_ordering_send_batch(
    seqs: Vec<u32>,
    state: State<'_, OrderingState>,
) -> Result<(), String> {
    enqueue(&state, &seqs)
}

#[tauri::command]
pub async fn spike_ordering_finish(
    expected: u32,
    state: State<'_, OrderingState>,
) -> Result<OrderingStats, String> {
    let run = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("no run in progress")?;
    drop(run.tx); // the writer drains the queue, then shuts the socket down
    run.writer
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let seqs = run
        .reader
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(stats(&seqs, expected))
}

fn stats(seqs: &[u32], expected: u32) -> OrderingStats {
    let mut seen = vec![0u32; expected as usize];
    let mut inversions = 0;
    let mut max_displacement = 0;
    let mut examples = Vec::new();
    let mut highest: Option<u32> = None;
    for &seq in seqs {
        if let Some(slot) = seen.get_mut(seq as usize) {
            *slot += 1;
        }
        match highest {
            Some(h) if seq < h => {
                inversions += 1;
                max_displacement = max_displacement.max(h - seq);
                if examples.len() < 5 {
                    examples.push((h, seq));
                }
            }
            _ => highest = Some(seq),
        }
    }
    OrderingStats {
        received: seqs.len(),
        inversions,
        max_displacement,
        missing: seen.iter().filter(|&&n| n == 0).count(),
        duplicates: seen.iter().filter(|&&n| n > 1).count(),
        examples,
    }
}

#[cfg(test)]
mod tests {
    use super::stats;

    #[test]
    fn counts_inversions_and_gaps() {
        let s = stats(&[0, 2, 1, 3, 3], 5);
        assert_eq!(s.received, 5);
        assert_eq!(s.inversions, 1);
        assert_eq!(s.max_displacement, 1);
        assert_eq!(s.missing, 1); // 4
        assert_eq!(s.duplicates, 1); // 3
        assert_eq!(s.examples, vec![(2, 1)]);
    }

    #[test]
    fn in_order_stream_is_clean() {
        let s = stats(&(0..100).collect::<Vec<_>>(), 100);
        assert_eq!((s.inversions, s.missing, s.duplicates), (0, 0, 0));
    }
}
