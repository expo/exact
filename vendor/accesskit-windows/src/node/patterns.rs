// Copyright 2022 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

// Derived from Chromium's accessibility abstraction.
// Copyright 2021 The Chromium Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE.chromium file.

macro_rules! properties {
    ($(($id:ident, $m:ident)),+) => {
        impl NodeWrapper<'_> {
            fn get_property_value(&mut self, property_id: UIA_PROPERTY_ID) -> Variant {
                match property_id {
                    $($id => {
                        self.$m().into()
                    })*
                    _ => Variant::empty()
                }
            }
            fn enqueue_simple_property_changes(
                &mut self,
                queue: &mut Vec<QueuedEvent>,
                context: &Arc<Context>,
                element: &IRawElementProviderSimple,
                old: &mut NodeWrapper,
            ) {
                $({
                    let old_value = old.$m();
                    let new_value = self.$m();
                    if old_value != new_value {
                        enqueue_property_change(
                            queue,
                            element,
                            $id,
                            old_value.into(),
                            new_value.into(),
                        );
                    }
                })*

                let mut old_controls = old.node.controls().filter(|controlled| filter(controlled) == FilterResult::Include);
                let mut new_controls = self.node.controls().filter(|controlled| filter(controlled) == FilterResult::Include);
                let mut are_equal = true;
                let mut controls: Vec<IUnknown> = Vec::new();
                loop {
                    let old_controlled = old_controls.next();
                    let new_controlled = new_controls.next();
                    match (old_controlled, new_controlled) {
                        (Some(a), Some(b)) => {
                            are_equal = are_equal && a.id() == b.id();
                            controls.push(context.get_or_create_platform_node(b.id()).into_interface::<IRawElementProviderSimple>().into());
                        }
                        (None, None) => break,
                        _ => are_equal = false,
                    }
                }
                if !are_equal {
                    enqueue_property_change(
                        queue,
                        &element,
                        UIA_ControllerForPropertyId,
                        Variant::empty(),
                        controls.into(),
                    );
                }
            }
        }
    };
}

macro_rules! patterns {
    ($(($pattern_id:ident, $provider_interface:ident, $provider_interface_impl:ident, $is_supported:ident, (
        $(($property_id:ident, $com_getter:ident, $getter:ident, $com_type:ident)),*
    ), (
        $($extra_trait_method:item),*
    ))),+) => {
        impl PlatformNode_Impl {
            fn pattern_provider(&self, pattern_id: UIA_PATTERN_ID) -> Result<IUnknown> {
                self.resolve(|node| {
                    let mut buffer = StringBuffer::acquire();
                    let wrapper = NodeWrapper { node: &node, string_buffer: &mut buffer };
                    match pattern_id {
                        $($pattern_id if wrapper.$is_supported() => {
                            let intermediate: $provider_interface = self.to_interface();
                            return intermediate.cast();
                        })*
                        _ => (),
                    }
                    Err(Error::empty())
                })
            }
        }
        impl NodeWrapper<'_> {
            fn enqueue_pattern_property_changes(
                &mut self,
                queue: &mut Vec<QueuedEvent>,
                element: &IRawElementProviderSimple,
                old: &mut NodeWrapper,
            ) {
                $(if self.$is_supported() && old.$is_supported() {
                    $({
                        let old_value = old.$getter();
                        let new_value = self.$getter();
                        if old_value != new_value {
                            enqueue_property_change(
                                queue,
                                element,
                                $property_id,
                                old_value.into(),
                                new_value.into(),
                            );
                        }
                    })*
                })*
            }
        }
        $(#[allow(non_snake_case)]
        impl $provider_interface_impl for PlatformNode_Impl {
            $(fn $com_getter(&self) -> Result<$com_type> {
                self.resolve(|node| {
                    let mut buffer = StringBuffer::acquire();
                    #[allow(unused_mut)]
                    let mut wrapper = NodeWrapper { node: &node, string_buffer: &mut buffer };
                    Ok(wrapper.$getter().into())
                })
            })*
            $($extra_trait_method)*
        })*
    };
}
