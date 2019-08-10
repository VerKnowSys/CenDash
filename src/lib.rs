#![recursion_limit = "768"]


#[macro_use]
extern crate stdweb;

#[macro_use]
extern crate serde_derive;


use failure::Error;
use std::time::Duration;
use yew::format::nothing::Nothing;
use yew::format::Json;
use yew::services::{
    fetch::{FetchService, Request, Response},
    ConsoleService, IntervalService, Task, StorageService // TimeoutService, DialogService,
};
use yew::{
    html, ChangeData, Callback, Component, ComponentLink, Html, Renderable, ShouldRender
};
use yew::services::storage::Area;


const INVENTORY_FILE: &'static str = "/inventory";
const DATASTORE_BROWSER_ID: &'static str = "cendash-data-store";


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

    // serializable data
    data: CenDashData,
}


#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CenDashData {

    pub gitref: String,

    pub messages: Vec<String>,

    pub hosts_all: Vec<String>,

    pub hosts_picked: Vec<String>,

    pub inventory: Vec<String>,

    pub logs: Vec<String>,

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

            data: CenDashData::default(),
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
                self.data.hosts_all
                    = self
                        .data
                        .inventory
                        .clone();
                self.data.hosts_picked
                    = self
                        .data
                        .inventory
                        .clone();
                self.console.info(&format!("Inventory loaded with {} hosts!", self.inventory.len()));
                self.console.debug(&format!("Inventory data: {:?}", data));
                self.job = None;
                self.job_onload = None; // disable job_onload after initial call
            }

            Msg::Deploy => {
                if self.data.gitref.len() > 3
                && self.data.inventory.len() > 0 {
                    let handle
                        = self
                            .interval
                            .spawn(Duration::from_millis(300), self.callback_deploy.clone());
                    self.job = Some(Box::new(handle));

                    self.data.messages.clear();
                    self.console.clear();
                    self.console.log(&format!("GitRef: {}", &self.data.gitref));
                    self.console.log(&format!("Picked hosts: {:?}", &self.data.hosts_picked));

                } else {
                    self.data.messages.push(format!("No GitRef given!"));
                }
            }

            Msg::Abort => {
                if let Some(mut task) = self.job.take() {
                    task.cancel();
                }
                self.data.messages.push(format!("Aborted!"));
                self.console.warn(&format!("Aborted!"));
                self.console.assert(self.job.is_none(), "Job still exists!");
            }

            Msg::Done => {
                self.data.messages.push(format!("Done!"));
                self.console.info("Done!");
                // self.console.group();
                // self.console.time_named_end("Timer");
                // self.console.group_end();
                self.job = None;
            }

            Msg::InvokeProcess => {
                self.data.messages.push(format!("DeploySteps!"));
                self.console.count_named(&format!("DeploySteps GitRef: {}", self.data.gitref));
                // Job's done:
                {
                    let handle = self
                        .timeout
                        .spawn(Duration::from_secs(3), self.callback_done.clone());
                    self.job = Some(Box::new(handle));
                }
            }

            Msg::SetGitRef(gitref) => {
                self.data.gitref = gitref.to_string();
            }

            Msg::SetOrUnsetHost(data) => {
                match data {
                    ChangeData::Select(hosts) => {
                        self.data.hosts_picked = hosts.selected_values();
                        self.console.log(&format!("Hosts Select(hosts): {:?}", self.data.hosts_picked));
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
                            value=&self.data.gitref
                            oninput=|element| Msg::SetGitRef(element.value)
                        />
                    </pre>
                    <pre>
                        { "Selected: " }
                        { self.data.hosts_picked.len() }
                        { " of: " }
                        { self.data.hosts_all.len() }
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
                    { for self.data.messages.iter().map(view_message) }
                </content>
            </article>
        }
    }
}
