// SPDX-FileCopyrightText: 2021 Softbear, Inc.
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::component::positioner::Position;
use crate::frontend::{post_message, use_change_common_settings_callback, use_ctw};
use crate::translation::{use_translation, Translation};
use crate::WindowEventListener;
use core_protocol::name::PlayerAlias;
use gloo::timers::callback::Timeout;
use stylist::yew::styled_component;
use web_sys::{AnimationEvent, HtmlInputElement, MessageEvent, SubmitEvent};
use yew::prelude::*;

#[derive(PartialEq, Properties)]
pub struct DialogProps {
    pub on_play: Callback<PlayerAlias>,
    #[prop_or(Position::Center)]
    pub position: Position,
    pub children: Children,
    // Kiomet used: #22222288
    #[prop_or("#00000025")]
    pub input_background_color: &'static str,
}

#[styled_component(SpawnOverlay)]
pub fn spawn_overlay(props: &DialogProps) -> Html {
    let form_style = css!(
        r#"
        position: relative;
        row-gap: 2rem;
        user-select: none;
        min-width: 50%;
        animation: fadein 1s;

        @keyframes fadein {
            from { opacity: 0; }
            to   { opacity: 1; }
        }
    "#
    );

    let input_style = css!(
        r#"
        background: #75aaff;
        border-radius: 3rem;
        border: 0;
        box-sizing: border-box;
        color: white;
        cursor: pointer;
        font-size: 1.5rem;
        font-weight: bold;
        outline: 0;
        text-align: center;
        padding: 0.7em;
        width: 58%;
        float: left;
        position: absolute;
        left: 0;
        top: 85%;
        margin-left: 7%;
        box-shadow: inset 0px -0.4rem 0px 0px #00000025;
        transition: box-shadow 500ms ease;

        ::placeholder {
            color: white;
            opacity: 1;
        }
        .input::-ms-input-placeholder {
            color: white;
        }
        .input:hover {
            box-shadow:none;
        }
   "#
    );

    let button_style = css!(
        r#"
        background: #28af71;
        border-radius: 3rem;
        border: 0;
        box-sizing: border-box;
        color: white;
        cursor: pointer;
        font-size: 1.5rem;
        font-weight: bold;
        outline: 0;
        text-align: center;
        padding: 0.7em;
        width: 30%;
        display: inline;
        float: right;
        position: absolute;
        right: 0;
        top: 85%;
        box-shadow: inset 0px -0.4rem 0px 0px #00000025;
        transition: box-shadow 500ms ease;

        :hover {
            box-shadow: none;
        }
    "#
    ); //edited

    let t = use_translation();
    let (paused, transitioning, onanimationend) = use_splash_screen();
    let alias_setting = use_ctw().setting_cache.alias;
    let input_ref = use_node_ref();

    let onplay = {
        let input_ref = input_ref.clone();
        let setting_callback = use_change_common_settings_callback();
        props.on_play.reform(move |_| {
            let alias = input_ref
                .cast::<HtmlInputElement>()
                .map(|input| PlayerAlias::new_input_sanitized(&input.value()));
            setting_callback.emit(Box::new(move |settings, storages| {
                settings.set_alias(alias, storages);
            }));
            alias.unwrap_or_default()
        })
    };

    let onclick = onplay.reform(|_: MouseEvent| {});

    let onsubmit = onplay.reform(|event: SubmitEvent| {
        event.prevent_default();
    });

    {
        let input_ref = input_ref.clone();
        use_effect_with_deps(
            move |alias_setting| {
                if let Some(alias_setting) = alias_setting.as_ref() {
                    if let Some(input) = input_ref.cast::<HtmlInputElement>() {
                        input.set_value(&alias_setting);
                    }
                }
            },
            alias_setting,
        );
    }

    html! {
        <form id="spawn_overlay" class={form_style} style={props.position.to_string()} {onsubmit} {onanimationend}>
            {props.children.clone()}
            <input
                ref={input_ref}
                id="alias_input"
                class={input_style}
                disabled={*transitioning}
                type="text"
                minlength="1"
                maxlength="12"
                placeholder={t.splash_screen_alias_placeholder()}
                autocomplete="off"
            />
            <button
                id="play_button"
                class={button_style}
                disabled={*paused || *transitioning}
                {onclick}
            >{t.splash_screen_play_label()}</button>
            <div id="banner_bottom" style="margin: auto;"></div>
        </form>
    }
}

/// Should be called on game-specific respawn screens.
#[hook]
pub fn use_splash_screen() -> (
    UseStateHandle<bool>,
    UseStateHandle<bool>,
    Option<Callback<AnimationEvent>>,
) {
    let paused = use_state(|| false);
    let transitioning = use_state(|| true);

    let onanimationend = transitioning.then(|| {
        let transitioning = transitioning.clone();
        Callback::from(move |_| {
            post_message("splash");
            transitioning.set(false);
        })
    });

    {
        let paused = paused.clone();
        let transitioning = transitioning.clone();

        // See https://yew.rs/docs/concepts/function-components/pre-defined-hooks for why dep is
        // needed.
        let transitioning_dep = *transitioning;

        use_effect_with_deps(
            |currently_transitioning| {
                let not_transitioning = !*currently_transitioning;
                let listener = WindowEventListener::new(
                    "message",
                    move |event: &MessageEvent| {
                        if let Some(message) = event.data().as_string() {
                            match message.as_str() {
                                "pause" => paused.set(true),
                                "unpause" => paused.set(false),
                                "snippetLoaded" if not_transitioning => post_message("splash"),
                                _ => {}
                            }
                        }
                    },
                    false,
                );

                // Defend against css animation end event not firing.
                let transition_timeout = not_transitioning
                    .then_some(Timeout::new(1500, move || transitioning.set(false)));

                || {
                    drop(listener);
                    drop(transition_timeout);
                }
            },
            transitioning_dep,
        );
    }

    use_effect_with_deps(
        |_| {
            // No-op.
            || {
                // Send this when unmounting.
                post_message("playing");
            }
        },
        (),
    );

    (paused, transitioning, onanimationend)
}
