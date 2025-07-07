use std::collections::{HashMap, HashSet};
use std::io::stderr;
use std::net::SocketAddr;
use std::sync::Arc;

use bars_protocol::SceneryObject;

use anyhow::Result;

use clap::Parser;

use futures::{SinkExt, StreamExt};

use hyper::body::Incoming;
use hyper::server::conn::http1 as conn;
use hyper::service::service_fn;
use hyper::{header, Method, Request, Response, StatusCode, Version};

use hyper_util::rt::TokioIo;

use serde_json::{json, Value};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::broadcast::Sender;
use tokio::sync::Mutex;

use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Message, Role};
use tokio_tungstenite::WebSocketStream;

use tracing::{debug, error, info, instrument, warn};

use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::ChronoUtc;
use tracing_subscriber::FmtSubscriber;

type Downstream = bars_protocol::Downstream<Value>;
type Upstream = bars_protocol::Upstream<Value>;

/// Serve a local version of the BARS server.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
	/// accept KEY as a controller API key
	#[arg(short = 'c', long = "controller", value_name = "KEY")]
	controller_keys: Vec<String>,

	/// accept KEY as an observer API key
	#[arg(short = 'o', long = "observer", value_name = "KEY")]
	observer_keys: Vec<String>,

	/// bind server to ADDRESS
	#[arg(value_name = "ADDRESS")]
	bind: SocketAddr,
}

struct Config {
	controller_keys: HashSet<String>,
	observer_keys: HashSet<String>,
}

type State = HashMap<String, StateEntry>;

#[derive(Clone)]
struct StateEntry {
	aerodrome: Arc<Mutex<Aerodrome>>,
	broadcast: Sender<Downstream>,
}

impl Default for StateEntry {
	fn default() -> Self {
		Self {
			aerodrome: Default::default(),
			broadcast: Sender::new(16),
		}
	}
}

#[derive(Clone, Default)]
struct Aerodrome {
	controllers: HashSet<String>,
	objects: HashMap<String, bool>,
	state: Value,
}

impl Aerodrome {
	fn merge_state(&mut self, state: Value) {
		fn merge(target: &mut Value, source: Value) {
			if target.is_object() && source.is_object() {
				let Value::Object(target) = target else {
					unreachable!()
				};
				let Value::Object(source) = source else {
					unreachable!()
				};

				for (key, value) in source {
					if let Some(target) = target.get_mut(&key) {
						merge(target, value);
					} else {
						target.insert(key, value);
					}
				}
			} else {
				*target = source;
			}
		}

		merge(&mut self.state, state);
	}
}

#[tokio::main]
async fn main() -> Result<()> {
	let args = Args::parse();

	let subscriber = FmtSubscriber::builder()
		.with_ansi(true)
		.with_level(true)
		.with_max_level(LevelFilter::TRACE)
		.with_timer(ChronoUtc::new("%TZ".into()))
		.with_writer(stderr)
		.finish();

	tracing::subscriber::set_global_default(subscriber)?;

	info!("logging initialised");

	let listener = TcpListener::bind(args.bind).await?;

	let config: &'static _ = Box::leak(Box::new(Config {
		controller_keys: HashSet::from_iter(args.controller_keys),
		observer_keys: HashSet::from_iter(args.observer_keys),
	}));
	let state = Arc::new(Mutex::new(State::new()));

	if !config.controller_keys.is_disjoint(&config.observer_keys) {
		warn!("overlapping controller and observer keys");
	}

	loop {
		let (stream, remote) = listener.accept().await?;

		let stream = TokioIo::new(stream);
		let id = remote.to_string();
		let state = state.clone();

		debug!("accepted {remote}");

		tokio::spawn(async move {
			let service =
				service_fn(move |req| handle(req, id.clone(), config, state.clone()));
			let conn = conn::Builder::new()
				.serve_connection(stream, service)
				.with_upgrades();

			if let Err(err) = conn.await {
				error!("failed to serve: {err}");
			} else {
				debug!("closed {remote}");
			}
		});
	}
}

#[instrument(skip_all)]
async fn handle(
	req: Request<Incoming>,
	id: String,
	config: &Config,
	state: Arc<Mutex<State>>,
) -> Result<Response<String>> {
	debug!("{} {}", req.method(), req.uri().path());

	Ok(match req.uri().path() {
		"/connect" => {
			let params = get_websocket_request(&req).zip(req.uri().query()).and_then(
				|(accept_key, query)| {
					let params = query
						.split('&')
						.filter_map(|tuple| tuple.split_once('='))
						.collect::<HashMap<_, _>>();
					params
						.get("airport")
						.copied()
						.zip(params.get("key").copied())
						.map(|params| (accept_key, params))
				},
			);

			if let Some((accept_key, (icao, key))) = params {
				let controller = config.controller_keys.contains(key);
				let observer = config.observer_keys.contains(key);

				if controller || observer {
					let state = state.clone();
					let icao = icao.to_string();

					tokio::spawn(async move {
						match hyper::upgrade::on(req).await {
							Ok(stream) => {
								let entry = {
									let mut state = state.lock().await;
									let state = state.entry(icao.clone()).or_default();

									if controller {
										let mut aerodrome = state.aerodrome.lock().await;
										aerodrome.controllers.insert(id.clone());

										let _ =
											state.broadcast.send(Downstream::ControllerConnect {
												controller_id: id.clone(),
											});
									}

									state.clone()
								};

								let stream = TokioIo::new(stream);
								let conn =
									WebSocketStream::from_raw_socket(stream, Role::Server, None)
										.await;

								let id_opt = controller.then_some(&id);

								if let Err(err) = handle_socket(conn, id_opt, entry).await {
									error!("handling error: {err}");
								}

								if controller {
									let state = state.lock().await;
									let state = state.get(&icao).unwrap();
									let mut aerodrome = state.aerodrome.lock().await;

									if aerodrome.controllers.remove(&id)
										&& aerodrome.controllers.is_empty()
									{
										aerodrome.objects.clear();
										aerodrome.state = Value::Null;
									}

									let _ =
										state.broadcast.send(Downstream::ControllerDisconnect {
											controller_id: id.clone(),
										});
								}
							},
							Err(err) => error!("failed to upgrade: {err}"),
						}
					});

					Response::builder()
						.status(StatusCode::SWITCHING_PROTOCOLS)
						.header(header::CONNECTION, "upgrade")
						.header(header::UPGRADE, "websocket")
						.header(header::SEC_WEBSOCKET_ACCEPT, accept_key)
						.body("".into())?
				} else {
					Response::builder()
						.status(StatusCode::UNAUTHORIZED)
						.body("unauthorized".into())?
				}
			} else {
				Response::builder()
					.status(StatusCode::BAD_REQUEST)
					.body("bad request".into())?
			}
		},
		"/state" => {
			let icao = (req.method() == Method::GET)
				.then_some(req.uri().query())
				.flatten()
				.and_then(|query| {
					query
						.split('&')
						.filter_map(|tuple| tuple.split_once('='))
						.find_map(|(k, v)| (k == "airport").then_some(v))
				});

			if let Some(icao) = icao {
				let state = state.lock().await;
				let aerodrome = if let Some(state) = state.get(icao) {
					let aerodrome = state.aerodrome.lock().await;
					aerodrome.clone()
				} else {
					Aerodrome::default()
				};

				let objects = aerodrome
					.objects
					.iter()
					.map(|(id, state)| {
						json!({
							"id": id,
							"state": state,
						})
					})
					.collect::<Vec<_>>();

				Response::builder()
					.header(header::CONTENT_TYPE, "application/json")
					.body(serde_json::to_string(&json!({
						"airport": icao,
						"controllers": aerodrome.controllers,
						"pilots": [],
						"objects": objects,
						"offline": aerodrome.controllers.is_empty(),
					}))?)?
			} else {
				Response::builder()
					.status(StatusCode::BAD_REQUEST)
					.body("bad request".into())?
			}
		},
		path => {
			warn!("not found: {path}");

			Response::builder()
				.status(StatusCode::NOT_FOUND)
				.body("not found".into())?
		},
	})
}

fn get_websocket_request(req: &Request<Incoming>) -> Option<String> {
	let is_websocket_request = req.method() == Method::GET
		&& req.version() >= Version::HTTP_11
		&& req
			.headers()
			.get(header::CONNECTION)
			.and_then(|v| v.to_str().ok())
			.map(|v| v.eq_ignore_ascii_case("upgrade"))
			.unwrap_or(false)
		&& req
			.headers()
			.get(header::UPGRADE)
			.and_then(|v| v.to_str().ok())
			.map(|v| {
				v.split([' ', ','])
					.any(|protocol| protocol.eq_ignore_ascii_case("websocket"))
			})
			.unwrap_or(false)
		&& req
			.headers()
			.get(header::SEC_WEBSOCKET_VERSION)
			.map(|v| v == "13")
			.unwrap_or(false);

	is_websocket_request
		.then(|| req.headers().get(header::SEC_WEBSOCKET_KEY))
		.flatten()
		.map(|key| derive_accept_key(key.as_bytes()))
}

#[instrument(skip_all)]
async fn handle_socket<S>(
	mut conn: WebSocketStream<S>,
	controller: Option<&String>,
	state: StateEntry,
) -> Result<()>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	async fn send<S>(
		conn: &mut WebSocketStream<S>,
		message: &Downstream,
	) -> Result<()>
	where
		S: AsyncRead + AsyncWrite + Unpin,
	{
		let message = serde_json::to_string(message).unwrap();
		if let Err(err) = conn.send(message.into()).await {
			error!("failed to send websocket message: {err}");

			let _ = conn.close(None).await;

			Err(err)?
		} else {
			Ok(())
		}
	}

	let tx = state.broadcast;
	let mut rx = tx.subscribe();

	{
		let aerodrome = state.aerodrome.lock().await;

		send(
			&mut conn,
			&Downstream::InitialState {
				connection_type: controller
					.map(|_| "controller")
					.unwrap_or("observer")
					.into(),
				scenery: aerodrome
					.objects
					.iter()
					.map(|(id, state)| SceneryObject {
						id: id.clone(),
						state: *state,
					})
					.collect(),
				patch: aerodrome.state.clone(),
			},
		)
		.await?;
	}

	loop {
		tokio::select! {
			Ok(message) = rx.recv() => {
				send(&mut conn, &message).await?;
			},
			message = conn.next() => {
				match message {
					Some(Ok(Message::Text(message))) => {
						let Ok(message) = serde_json::from_str(&message) else {
							send(&mut conn, &Downstream::Error {
								message: "malformed message".into(),
							}).await?;

							continue
						};

						match (message, controller) {
							(Upstream::Heartbeat, _) =>
								send(&mut conn, &Downstream::HeartbeatAck).await?,
							(Upstream::HeartbeatAck, _) => warn!("unexpected HEARTBEAT_ACK"),
							(Upstream::Close, _) => {
								debug!("closing websocket");

								conn.close(None).await?;

								break
							},
							(Upstream::StateUpdate { object_id, state: os }, Some(id)) => {
								let mut aerodrome = state.aerodrome.lock().await;
								aerodrome.objects.insert(object_id.clone(), os);

								let _ = tx.send(Downstream::StateUpdate {
									object_id,
									state: os,
									controller_id: id.clone(),
								});
							},
							(Upstream::SharedStateUpdate { patch }, Some(id)) => {
								let mut aerodrome = state.aerodrome.lock().await;
								aerodrome.merge_state(patch.clone());

								let _ = tx.send(Downstream::SharedStateUpdate {
									patch, controller_id: id.clone(),
								});
							},
							_ => send(&mut conn, &Downstream::Error {
								message: "invalid message".into(),
							}).await?,
						}
					},
					Some(Ok(Message::Close(_))) | None => {
						warn!("unexpected websocket close");

						break
					},
					Some(Ok(Message::Binary(_) | Message::Frame(_))) => {
						warn!("non-text message received");

						send(&mut conn, &Downstream::Error {
							message: "invalid websocket frame".into(),
						}).await?;
					},
					Some(Ok(Message::Ping(_) | Message::Pong(_))) => (),
					Some(Err(err)) => {
						error!("websocket error: {err}");

						let _ = conn.close(None).await;

						break
					},
				}
			},
		}
	}

	Ok(())
}
