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
    ConsoleService, IntervalService, Task, StorageService, TimeoutService //, DialogService,
};
use yew::{
    html, ChangeData, Callback, Component, ComponentLink, Html, Renderable, ShouldRender
};
use yew::services::storage::Area;
use regex::Regex;


const INVENTORY_FILE: &'static str = "/inventory";
const DATASTORE_BROWSER_ID: &'static str = "cendash-data-store";


pub struct Model {
    link: ComponentLink<Model>,

    timeout: TimeoutService,
    interval: IntervalService,
    console: ConsoleService,
    fetch_service: FetchService,
    local_storage: StorageService,

    callback_deploy: Callback<()>,
    // callback_done: Callback<()>,

    job: Option<Box<dyn Task>>,
    job_onload: Option<Box<dyn Task>>,

    // serializable data
    data: CenDashData,
}


#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CenDashData {

    pub gitref: String,

    pub filter_content: String,

    pub messages: Vec<String>,

    pub hosts_all: Vec<String>,

    pub hosts_picked: Vec<String>,

    pub inventory: Vec<String>,

    pub logs: Vec<String>,

}


pub enum Msg {
    Abort,
    Done,
    DeploySteps,
    Deploy,
    SetGitRef(String),
    SetOrUnsetHost(ChangeData),
    InventoryFetching,
    InventoryLoad,
    InventoryLoaded(String),
    StoreData,
    RestoreData,
    SetContentFilter(String),
}


impl Model {


    /// store current state in browser:
    fn store_state(&mut self) {
        let data_to_store = Json(&self.data);
        self
            .local_storage
            .store(DATASTORE_BROWSER_ID, data_to_store);
        self
            .console
            .log(&format!("Stored state data"));
    }


    /// load last state from browser:
    fn restore_state(&mut self) {
        match self.local_storage.restore(DATASTORE_BROWSER_ID) {
            Json(Ok(data)) => {
                self.data = data;
                self.console.log(&format!("Restored app state!"));
            },

            Json(Err(_)) => {
                // self.store_state();
                // self.data = CenDashData::default();
                self.console.log(&format!("No app state!"))
            },
        }
    }


    /// schedule inventory reloading:
    fn autoload_inventory(&mut self) -> Option<Box<Task>> {
        let callback_onload
            = self
                .link
                .send_back(|_| Msg::InventoryLoad);
        let job_onload
            = self
                .interval
                .spawn(Duration::from_millis(500), callback_onload);
        Some(Box::new(job_onload))
    }


}


impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, mut link: ComponentLink<Self>) -> Self {
        let mut interval = IntervalService::new();
        let callback_onload = link.send_back(|_| Msg::InventoryLoad);
        let job_onload = interval.spawn(Duration::from_secs(0), callback_onload);

        Model {
            timeout: TimeoutService::new(),
            fetch_service: FetchService::new(),
            local_storage: StorageService::new(Area::Local), // or Area::Session
            console: ConsoleService::new(),
            callback_deploy: link.send_back(|_| Msg::DeploySteps),
            // callback_done: link.send_back(|_| Msg::Done),
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
                            move |response: Response<Result<String, Error>>| {
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
                self
                    .job = Some(Box::new(handle));
            }

            Msg::InventoryFetching => {
                self.console.log("Seeking /static/inventory…");
            }

            Msg::InventoryLoaded(data) => {
                self.data.inventory
                    = data
                        .split("\n")
                        .filter(|line| {
                            let regex = Regex::new(&self.data.filter_content).unwrap();
                            regex.is_match(&line)
                            && !line.is_empty()
                            && !line.starts_with(&"[")
                            && !line.ends_with(&"]")
                            && line != &"\n"
                        })
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

                self.console.info(&format!("Inventory loaded with {} hosts!", self.data.inventory.len()));
                self.job = None;
                self.job_onload = None; // disable job_onload after initial call
            }

            Msg::Deploy => {
                if self.data.gitref.len() > 3 { // && self.data.inventory.len() > 0
                    let handle
                        = self
                            .interval
                            .spawn(Duration::from_millis(300), self.callback_deploy.clone());
                    self.job = Some(Box::new(handle));

                    self.data.messages.clear();
                    self.console.clear();
                    self.console.log(&format!("GitRef: {}", &self.data.gitref));
                    // self.console.log(&format!("Picked hosts: {:?}", &self.data.hosts_picked));

                } else {
                    self.data.messages.push(format!("Wrong GitRef given!"));
                }
            }

            Msg::Abort => {
                if let Some(mut task) = self.job.take() {
                    task.cancel();
                }
                self.data.messages.push(format!("Aborted!"));
                self.console.warn(&format!("Aborted!"));
                self.store_state();
                // self.console.assert(self.job.is_none(), "Job still exists!");
            }

            Msg::Done => {
                self.data.messages.push(format!("Done!"));
                self.console.info("Done!");
                self.store_state();
                // self.console.group();
                // self.console.time_named_end("Timer");
                // self.console.group_end();
                self.job = None;
            }

            Msg::DeploySteps => {
                self.data.messages.push(format!("DeploySteps!"));
                self.console.count_named(&format!("DeploySteps GitRef: {}", self.data.gitref));
                self.store_state();

                // // Job's done:
                // {
                //     let handle = self
                //         .timeout
                //         .spawn(Duration::from_secs(3), self.callback_done.clone());
                //     self.job = Some(Box::new(handle));
                // }
            }

            Msg::SetGitRef(gitref) => {
                self.data.gitref = gitref.to_string();
                self.store_state();
                self.console.log(&format!("SetGitRef: {}", self.data.gitref));

                // reload inventory automatically:
                self.job_onload = self.autoload_inventory();
            }

            Msg::SetContentFilter(filter) => {
                self.data.filter_content = filter.to_string();
                self.store_state();
                self.console.log(&format!("SetContentFilter: {}", self.data.filter_content));

                // reload inventory automatically:
                self.job_onload = self.autoload_inventory();
            }

            Msg::SetOrUnsetHost(data) => {
                match data {
                    ChangeData::Select(hosts) => {
                        self.data.hosts_picked = hosts.selected_values();
                        self.store_state();
                        self.console.log(&format!("Hosts Selected: {}", self.data.hosts_picked.len()));
                    }

                    ChangeData::Value(host) => {
                        self.console.log(&format!("NoOp for Selected Host: {}", host));
                    }

                    ChangeData::Files(files) => {
                        self.console.log(&format!("NoOp for ChangeData::Files(_): {:?}", files));
                    }
                }
            }

            Msg::StoreData => {
                self.store_state();
            }

            Msg::RestoreData => {
                self.restore_state();
            }

        }
        true
    }
}

impl Renderable<Model> for Model {

    fn view(&self) -> Html<Self> {
        let view_message = |message| {
            html! {
                <p>
                    { message }
                </p>
            }
        };
        let has_job = self.job.is_some();

        let selected_option = |option| {
            html! {
                <option selected=true>
                    { option }
                </option>
            }
        };
        let unselected_option = |option| {
            html! {
                <option selected=false>
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
                        <label>
                            { "List of hosts: " }
                        </label>
                        <select
                            name="hosts"
                            size="42"
                            required=true
                            multiple=true
                            onchange=|option| Msg::SetOrUnsetHost(option)
                        >
                            { // handle selected/ unselected items on multi-list
                                for self.data.hosts_all.iter().map(|option| {
                                    if self.data.hosts_picked.contains(option) {
                                        selected_option(option)
                                    } else {
                                        unselected_option(option)
                                    }
                                })
                            }
                        </select>
                    </pre>
                    <pre>
                        <label>
                            { "Filter hosts: " }
                        </label>
                        <input
                            name="filter_content"
                            type="find"
                            size="32"
                            placeholder="Filter hosts by content"
                            value=&self.data.filter_content
                            oninput=|element| Msg::SetContentFilter(element.value)
                        />
                    </pre>
                    <pre>
                        <button
                            onclick=|_| Msg::StoreData>{ "Store-State" }
                        </button>
                        { "  " }
                        <button
                            onclick=|_| Msg::RestoreData>{ "Restore-State" }
                        </button>
                    </pre>
                    <pre>
                        <button
                            disabled=has_job
                            onclick=|_| Msg::Deploy>{ "Deploy!" }
                        </button>
                        { "  " }
                        <button
                            disabled=!has_job
                            onclick=|_| Msg::Abort>{ "Abort!" }
                        </button>
                    </pre>
                    <pre>
                        <button
                            onclick=|_| Msg::InventoryLoad>{ "Reload-Inventory" }
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
