#![recursion_limit = "512"]

#[macro_use]
extern crate stdweb;

use failure::Error;
use yew::format::nothing::Nothing;
use std::time::Duration;
use yew::services::fetch::{FetchService, Request, Response};
use yew::services::{ConsoleService, IntervalService, Task, TimeoutService};
use yew::{html, ChangeData, Callback, Component, ComponentLink, Html, Renderable, ShouldRender};
// use stdweb::unstable::TryFrom;
// use yew::events::ChangeData;
// use yew::services::reader::{File, FileChunk, FileData, ReaderService, ReaderTask};
// use yew::virtual_dom::VNode;
// use std::fs::read_to_string;


const INVENTORY_FILE: &'static str = "/inventory";


pub struct Model {
    link: ComponentLink<Model>,

    timeout: TimeoutService,
    interval: IntervalService,
    console: ConsoleService,
    fetch_service: FetchService,

    callback_tick: Callback<()>,
    callback_done: Callback<()>,

    job: Option<Box<dyn Task>>,
    job_onload: Option<Box<dyn Task>>,

    messages: Vec<&'static str>,
    hosts_all: Vec<String>,
    hosts_picked: Vec<String>,
    gitref: String,
    inventory: Vec<String>,
}

pub enum Msg {
    Abort,
    Done,
    InvokeProcess,
    Deploy,
    SetGitRef(String),
    SetOrUnsetHost(ChangeData),
    InventoryLoad,
    InventoryLoaded(String),
    InventoryFetching,
}


impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, mut link: ComponentLink<Self>) -> Self {
        let mut interval = IntervalService::new();
        let callback_onload = link.send_back(|_| Msg::InventoryLoad);
        let job_onload = interval.spawn(Duration::from_secs(1), callback_onload);

        Model {
            timeout: TimeoutService::new(),
            console: ConsoleService::new(),
            callback_tick: link.send_back(|_| Msg::InvokeProcess),
            callback_done: link.send_back(|_| Msg::Done),
            interval,
            link,

            job: None,
            job_onload: Some(Box::new(job_onload)),

            gitref: String::new(),
            messages: Vec::new(),
            inventory: Vec::new(),
            hosts_all: Vec::new(),
            hosts_picked: Vec::new(),
            fetch_service: FetchService::new(),
        }
    }


    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::InventoryLoad => {
                let request
                    = Request::get(INVENTORY_FILE)
                        .body(Nothing)
                        .unwrap();
                let callback
                    = self
                        .link
                        .send_back(
                            |response: Response<Result<String, Error>>| {
                                let (meta, data) = response.into_parts();
                                let inventory_data = data.unwrap_or_default();
                                if meta.status.is_success() {
                                    Msg::InventoryLoaded(inventory_data)
                                } else {
                                    Msg::InventoryFetching // not yet fetched
                                }
                            }
                        );

                let handle
                    = self
                        .fetch_service
                        .fetch(request, callback);
                self.job = Some(Box::new(handle));
            }

            Msg::InventoryFetching => {
                self.console.log("Seeking /static/inventory…");
            }

            Msg::InventoryLoaded(data) => {
                self.inventory
                    = data
                        .split("\n")
                        .filter(|line| !line.starts_with("[") && line != &"\n" && line.len() > 0)
                        .map(|line| line.split(" ").take(1).collect::<String>())
                        .collect();
                self.hosts_all
                    = self
                        .inventory
                        .clone();
                self.hosts_picked
                    = self
                        .inventory
                        .clone();
                self.console.info(&format!("Inventory loaded with {} hosts!", self.inventory.len()));
                self.console.debug(&format!("Inventory data: {:?}", data));
                self.job = None;
                self.job_onload = None; // disable job_onload after initial call
            }

            Msg::Deploy => {
                if self.gitref.len() > 3
                && self.inventory.len() > 0 {
                    let handle
                        = self
                            .interval
                            .spawn(Duration::from_millis(300), self.callback_tick.clone());
                    self.job = Some(Box::new(handle));

                    self.messages.clear();
                    self.console.clear();
                    self.console.log(&format!("GitRef: {}", &self.gitref));
                    self.console.log(&format!("Picked hosts: {:?}", &self.hosts_picked));

                } else {
                    self.messages.push("No GitRef given!");
                }
            }

            Msg::Abort => {
                if let Some(mut task) = self.job.take() {
                    task.cancel();
                }
                self.messages.push("Aborted!");
                self.console.warn("Aborted!");
                self.console.assert(self.job.is_none(), "Job still exists!");
            }

            Msg::Done => {
                self.messages.push("Done!");
                self.console.info("Done!");
                // self.console.group();
                // self.console.time_named_end("Timer");
                // self.console.group_end();
                self.job = None;
            }

            Msg::InvokeProcess => {
                self.messages.push("InvokeProcess!");
                self.console.count_named(&format!("InvokeProcess GitRef: {}", self.gitref));
                // Job's done:
                {
                    let handle = self
                        .timeout
                        .spawn(Duration::from_secs(3), self.callback_done.clone());
                    self.job = Some(Box::new(handle));
                }
            }

            Msg::SetGitRef(gitref) => {
                self.gitref = gitref.to_string();
            }

            Msg::SetOrUnsetHost(data) => {
                match data {
                    ChangeData::Select(hosts) => {
                        self.hosts_picked = hosts.selected_values();
                        self.console.log(&format!("Hosts Select(hosts): {:?}", self.hosts_picked));
                    }

                    ChangeData::Value(host) => {
                        self.console.log(&format!("NoOp for Selected Host: {}", host));
                    }

                    ChangeData::Files(files) => {
                        self.console.log(&format!("NoOp for ChangeData::Files(_): {:?}", files));
                    }
                }
            }
        }
        true
    }
}

impl Renderable<Model> for Model {
    fn view(&self) -> Html<Self> {
        let view_message = |message| {
            html! { <p>{ message }</p> }
        };
        let has_job = self.job.is_some();

        let select_option = |option| {
            html! {
                <option selected=true>
                    { option }
                </option>
            }
        };

        js! {
            // inject js routine to auto scroll contents to bottom:
            var element = document.getElementsByTagName("content");
            element.scrollTop = element.scrollHeight - element.clientHeight;
            document.body.scrollIntoView(false);

            // focus input box:
            // document.getElementsByTagName("input").focus();
            // document.getElementById("input").focus();
        };

        html! {
            <article>
                <span style="display: block; float: left; position: fixed; top: 2em; right: 2em;">
                    <label>
                        { "Centra Deployer" }
                    </label>
                    <pre>
                        <input
                            name="gitref"
                            size="42"
                            autofocus=true
                            required=true
                            placeholder="Git-ref (tag, branch or sha1)"
                            value=&self.gitref
                            oninput=|element| Msg::SetGitRef(element.value)
                        />
                    </pre>
                    <pre>
                        { "Selected: " }
                        { self.hosts_picked.len() }
                        { " of: " }
                        { self.hosts_all.len() }
                        { " hosts in total."}
                    </pre>
                    <pre>
                        <select
                            name="hosts"
                            size="42"
                            required=true
                            multiple=true
                            onchange=|option| Msg::SetOrUnsetHost(option)
                        >
                            { for self.hosts_all.iter().map(select_option) }
                        </select>
                    </pre>
                    <pre>
                        <button disabled=has_job onclick=|_| Msg::Deploy>{ "Deploy!" }
                        </button>
                    </pre>
                    <pre>
                        <button disabled=!has_job onclick=|_| Msg::Abort>{ "Abort!" }
                        </button>
                    </pre>
                    <pre>
                        <button disabled=has_job onclick=|_| Msg::InventoryLoad>{ "Reload inventory" }
                        </button>
                    </pre>
                </span>

                <content>
                    { for self.messages.iter().map(view_message) }
                </content>
            </article>
        }
    }
}
