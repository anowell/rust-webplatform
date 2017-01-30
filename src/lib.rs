#![allow(unused_unsafe)]

extern crate libc;

use std::ffi::{CString, CStr};
use std::{mem, fmt};
use std::str;
use std::borrow::ToOwned;
use std::ops::Deref;
use std::cell::RefCell;
use std::clone::Clone;
use std::rc::Rc;
use std::collections::HashSet;
use std::char;
use std::iter::IntoIterator;

mod webplatform {
    pub use emscripten_asm_const;
    pub use emscripten_asm_const_int;
}

trait Interop {
    fn as_int(self, _:&mut Vec<CString>) -> libc::c_int;
}

impl Interop for i32 {
    fn as_int(self, _:&mut Vec<CString>) -> libc::c_int {
        return self;
    }
}

impl<'a> Interop for &'a str {
    fn as_int(self, arena:&mut Vec<CString>) -> libc::c_int {
        let c = CString::new(self).unwrap();
        let ret = c.as_ptr() as libc::c_int;
        arena.push(c);
        return ret;
    }
}

impl<'a> Interop for *const libc::c_void {
    fn as_int(self, _:&mut Vec<CString>) -> libc::c_int {
        return self as libc::c_int;
    }
}

#[macro_export]
macro_rules! js {
    ( ($( $x:expr ),*) $y:expr ) => {
        {
            let mut arena:Vec<CString> = Vec::new();
            const LOCAL: &'static [u8] = $y;
            unsafe { ::webplatform::emscripten_asm_const_int(&LOCAL[0] as *const _ as *const libc::c_char, $(Interop::as_int($x, &mut arena)),*) }
        }
    };
    ( $y:expr ) => {
        {
            const LOCAL: &'static [u8] = $y;
            unsafe { ::webplatform::emscripten_asm_const_int(&LOCAL[0] as *const _ as *const libc::c_char) }
        }
    };
}

extern "C" {
    pub fn emscripten_asm_con(s: *const libc::c_char);
    pub fn emscripten_asm_const(s: *const libc::c_char);
    pub fn emscripten_asm_const_int(s: *const libc::c_char, ...) -> libc::c_int;
    pub fn emscripten_pause_main_loop();
    pub fn emscripten_set_main_loop(m: extern fn(), fps: libc::c_int, infinite: libc::c_int);
}

pub struct HtmlNode<'a> {
    id: libc::c_int,
    doc: *const Document<'a>,
}

impl<'a> fmt::Debug for HtmlNode<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "HtmlNode({:?})", self.id)
    }
}

impl<'a> Drop for HtmlNode<'a> {
    fn drop(&mut self) {
        println!("dropping HTML NODE {:?}", self.id);
    }
}

pub struct JSRef<'a> {
    ptr: *const HtmlNode<'a>,
}

impl<'a> fmt::Debug for JSRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "JSRef(HtmlNode({:?}))", self.id)
    }
}

impl<'a> Clone for JSRef<'a> {
    fn clone(&self) -> JSRef<'a> {
        JSRef {
            ptr: self.ptr,
        }
    }
}

impl<'a> HtmlNode<'a> {
    pub fn root_ref(&self) -> JSRef<'a> {
        JSRef {
            ptr: &*self,
        }
    }
}

impl<'a> Deref for JSRef<'a> {
    type Target = HtmlNode<'a>;

    fn deref(&self) -> &HtmlNode<'a> {
        unsafe {
            &*self.ptr
        }
    }
}

pub struct Event<'a> {
    pub target: Option<HtmlNode<'a>>
}

extern fn rust_caller<'a, F: FnMut(Event<'a>)>(a: *const libc::c_void, docptr: *const libc::c_void, id: i32) {
    let v:&mut F = unsafe { mem::transmute(a) };
    v(Event {
        target: if id == -1 {
            None
        } else {
            Some(HtmlNode {
                id: id,
                doc: unsafe { mem::transmute(docptr) },
            })
        }
        // target: None,
    });
}

impl<'a> HtmlNode<'a> {
    pub fn element_query(&self, s: &str) -> Option<HtmlNode<'a>> {
        let id = js! { (self.id, s) b"\
            var value = WEBPLATFORM.rs_refs[$0].querySelector(UTF8ToString($1));\
            if (!value) {\
                return -1;\
            }\
            return WEBPLATFORM.rs_refs.push(value) - 1;\
        \0" };

        if id < 0 {
            None
        } else {
            Some(HtmlNode {
                id: id,
                doc: self.doc,
            })
        }
    }

    pub fn element_query_all<'b>(&'b self, s: &str) -> Vec<HtmlNode<'a>> {
        let start: i32 = 0;
        let start_ptr: *const i32 = &start;
        let start_vptr = start_ptr as *const libc::c_void;
        let count = js! { (self.id, s, start_vptr) b"\
            var elements = WEBPLATFORM.rs_refs[$0].querySelectorAll(UTF8ToString($1));\
            if (elements.length == 0) {\
                return 0;\
            }\
            var prev_len = WEBPLATFORM.rs_refs.length;\
            setValue($2, prev_len, 'i32');\
            Array.prototype.push.apply(WEBPLATFORM.rs_refs, elements);\
            return elements.length;\
        \0" };
        (start..(start+count)).map(|id| HtmlNode{ id: id, doc: self.doc }).collect()
    }

    pub fn tagname(&self) -> String {
        let a = js! { (self.id) b"\
            var str = WEBPLATFORM.rs_refs[$0].tagName.toLowerCase();\
            return allocate(intArrayFromString(str), 'i8', ALLOC_STACK);\
        \0" };
        unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        }
    }

    pub fn focus(&self) {
        js! { (self.id) b"\
            WEBPLATFORM.rs_refs[$0].focus();\
        \0" };
    }

    pub fn html_set(&self, s: &str) {
        js! { (self.id, s) b"\
            WEBPLATFORM.rs_refs[$0].innerHTML = UTF8ToString($1);\
        \0" };
    }

    pub fn html_patch(&self, s: &str) {
        js! { (self.id, s) b"\
            var newTree = WEBPLATFORM.rs_refs[$0].cloneNode();\
            newTree.innerHTML = UTF8ToString($1);\
            morphdom(WEBPLATFORM.rs_refs[$0], newTree);\
        \0" };
    }

    pub fn html_get(&self) -> String {
        let a = js! { (self.id) b"\
            return allocate(intArrayFromString(WEBPLATFORM.rs_refs[$0].innerHTML), 'i8', ALLOC_STACK);\
        \0" };
        unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        }
    }

    pub fn class_get(&self) -> HashSet<String> {
        let a = js! { (self.id) b"\
            return allocate(intArrayFromString(WEBPLATFORM.rs_refs[$0].className), 'i8', ALLOC_STACK);\
        \0" };
        let class = unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        };
        class.trim().split(char::is_whitespace).map(|x| x.to_string()).collect()
    }

    pub fn class_add(&self, s: &str) {
        js! { (self.id, s) b"\
            WEBPLATFORM.rs_refs[$0].classList.add(UTF8ToString($1));\
        \0" };
    }

    pub fn class_remove(&self, s: &str) {
        js! { (self.id, s) b"\
            WEBPLATFORM.rs_refs[$0].classList.remove(UTF8ToString($1));\
        \0" };
    }

    pub fn parent(&self) -> Option<HtmlNode<'a>> {
        let id = js! { (self.id) b"\
            var value = WEBPLATFORM.rs_refs[$0].parentNode;\
            if (!value) {\
                return -1;\
            }\
            return WEBPLATFORM.rs_refs.push(value) - 1;\
        \0" };
        if id < 0 {
            None
        } else {
            Some(HtmlNode {
                id: id,
                doc: self.doc,
            })
        }
    }

    pub fn data_set(&self, s: &str, v: &str) {
        js! { (self.id, s, v) b"\
            WEBPLATFORM.rs_refs[$0].dataset[UTF8ToString($1)] = UTF8ToString($2);\
        \0" };
    }

    pub fn data_get(&self, s: &str) -> Option<String> {
        let a = js! { (self.id, s) b"\
            var str = WEBPLATFORM.rs_refs[$0].dataset[UTF8ToString($1)];\
            if (str == null) return -1;\
            return allocate(intArrayFromString(str), 'i8', ALLOC_STACK);\
        \0" };
        if a == -1 {
            None
        } else {
            Some(unsafe {
                str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
            })
        }
    }

    pub fn style_set_str(&self, s: &str, v: &str) {
        js! { (self.id, s, v) b"\
            WEBPLATFORM.rs_refs[$0].style[UTF8ToString($1)] = UTF8ToString($2);\
        \0" };
    }

    pub fn style_get_str(&self, s: &str) -> String {
        let a = js! { (self.id, s) b"\
            return allocate(intArrayFromString(WEBPLATFORM.rs_refs[$0].style[UTF8ToString($1)]), 'i8', ALLOC_STACK);\
        \0" };
        unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        }
    }

    pub fn prop_set_i32(&self, s: &str, v: i32) {
        js! { (self.id, s, v) b"\
            WEBPLATFORM.rs_refs[$0][UTF8ToString($1)] = $2;\
        \0" };
    }

    pub fn prop_set_str(&self, s: &str, v: &str) {
        js! { (self.id, s, v) b"\
            WEBPLATFORM.rs_refs[$0][UTF8ToString($1)] = UTF8ToString($2);\
        \0" };
    }

    pub fn prop_get_i32(&self, s: &str) -> i32 {
        return js! { (self.id, s) b"\
            return Number(WEBPLATFORM.rs_refs[$0][UTF8ToString($1)])\
        \0" };
    }

    pub fn prop_get_str(&self, s: &str) -> String {
        let a = js! { (self.id, s) b"\
            var a = allocate(intArrayFromString(WEBPLATFORM.rs_refs[$0][UTF8ToString($1)] || ''), 'i8', ALLOC_STACK); console.log(WEBPLATFORM.rs_refs[$0]); return a;\
        \0" };
        unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        }
    }

    pub fn attr_set_i32(&self, s: &str, v: i32) {
        js! { (self.id, s, v) b"\
            WEBPLATFORM.rs_refs[$0].setAttribute(UTF8ToString($1), $2);\
        \0" };
    }

    pub fn attr_set_str(&self, s: &str, v: &str) {
        js! { (self.id, s, v) b"\
            WEBPLATFORM.rs_refs[$0].setAttribute(UTF8ToString($1), UTF8ToString($2));\
        \0" };
    }

    pub fn attr_get_i32(&self, s: &str) -> i32 {
        return js! { (self.id, s) b"\
            return Number(WEBPLATFORM.rs_refs[$0].getAttribute(UTF8ToString($1)))\
        \0" };
    }

    pub fn attr_get_str(&self, s: &str) -> String {
        let a = js! { (self.id, s) b"\
            var a = allocate(intArrayFromString(WEBPLATFORM.rs_refs[$0].getAttribute(UTF8ToString($1)) || ''), 'i8', ALLOC_STACK); console.log(WEBPLATFORM.rs_refs[$0]); return a;\
        \0" };
        unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        }
    }

    pub fn append(&self, s: &HtmlNode) {
        js! { (self.id, s.id) b"\
            WEBPLATFORM.rs_refs[$0].appendChild(WEBPLATFORM.rs_refs[$1]);\
        \0" };
    }

    pub fn html_append(&self, s: &str) {
        js! { (self.id, s) b"\
            WEBPLATFORM.rs_refs[$0].insertAdjacentHTML('beforeEnd', UTF8ToString($1));\
        \0" };
    }

    pub fn html_prepend(&self, s: &str) {
        js! { (self.id, s) b"\
            WEBPLATFORM.rs_refs[$0].insertAdjacentHTML('afterBegin', UTF8ToString($1));\
        \0" };
    }

    pub fn on<F: FnMut(Event<'a>) + 'a>(&self, s: &str, f: F) {
        unsafe {
            let b = Box::new(f);
            let a = &*b as *const _;
            js! { (self.id, s, a as *const libc::c_void,
                rust_caller::<F> as *const libc::c_void,
                self.doc as *const libc::c_void)
                b"\
                WEBPLATFORM.rs_refs[$0].addEventListener(UTF8ToString($1), function (e) {\
                    Runtime.dynCall('viii', $3, [$2, $4, e.target ? WEBPLATFORM.rs_refs.push(e.target) - 1 : -1]);\
                }, false);\
            \0" };
            (&*self.doc).refs.borrow_mut().push(b);
        }
    }

    pub fn captured_on<F: FnMut(Event<'a>) + 'a>(&self, s: &str, f: F) {
        unsafe {
            let b = Box::new(f);
            let a = &*b as *const _;
            js! { (self.id, s, a as *const libc::c_void,
                rust_caller::<F> as *const libc::c_void,
                self.doc as *const libc::c_void)
                b"\
                WEBPLATFORM.rs_refs[$0].addEventListener(UTF8ToString($1), function (e) {\
                    Runtime.dynCall('viii', $3, [$2, $4, e.target ? WEBPLATFORM.rs_refs.push(e.target) - 1 : -1]);\
                }, true);\
            \0" };
            (&*self.doc).refs.borrow_mut().push(b);
        }
    }

    pub fn remove_self(&self) {
        js! { (self.id) b"\
            var s = WEBPLATFORM.rs_refs[$0];\
            s.parentNode.removeChild(s);\
        \0" };
    }
}

pub fn alert(s: &str) {
    js! { (s) b"\
        alert(UTF8ToString($0));\
    \0" };
}

pub struct Document<'a> {
    refs: Rc<RefCell<Vec<Box<FnMut(Event<'a>) + 'a>>>>,
}

impl<'a> Document<'a> {
    pub fn element_create<'b>(&'b self, s: &str) -> Option<HtmlNode<'a>> {
        let id = js! { (s) b"\
            var value = document.createElement(UTF8ToString($0));\
            if (!value) {\
                return -1;\
            }\
            return WEBPLATFORM.rs_refs.push(value) - 1;\
        \0" };

        if id < 0 {
            None
        } else {
            Some(HtmlNode {
                id: id,
                doc: &*self,
            })
        }
    }

    pub fn location_hash_get(&self) -> String {
        let a = js! { b"\
            return allocate(intArrayFromString(window.location.hash), 'i8', ALLOC_STACK);\
        \0" };
        unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        }
    }

    pub fn on<F: FnMut(Event) + 'a>(&self, s: &str, f: F) {
        unsafe {
            let b = Box::new(f);
            let a = &*b as *const _;
            js! { (0, s, a as *const libc::c_void,
                rust_caller::<F> as *const libc::c_void,
                &*self as *const _ as *const libc::c_void)
                b"\
                window.addEventListener(UTF8ToString($1), function (e) {\
                    Runtime.dynCall('viii', $3, [$2, $4, e.target ? WEBPLATFORM.rs_refs.push(e.target) - 1 : -1]);\
                }, false);\
            \0" };
            self.refs.borrow_mut().push(b);
        }
    }

    pub fn element_query<'b>(&'b self, s: &str) -> Option<HtmlNode<'a>> {
        let id = js! { (s) b"\
            var value = document.querySelector(UTF8ToString($0));\
            if (!value) {\
                return -1;\
            }\
            return WEBPLATFORM.rs_refs.push(value) - 1;\
        \0" };

        if id < 0 {
            None
        } else {
            Some(HtmlNode {
                id: id,
                doc: self,
            })
        }
    }

    pub fn element_query_all<'b>(&'b self, s: &str) -> Vec<HtmlNode<'a>> {
        let start: i32 = 0;
        let start_ptr: *const i32 = &start;
        let start_vptr = start_ptr as *const libc::c_void;
        let count = js! { (s, start_vptr) b"\
            var elements = document.querySelectorAll(UTF8ToString($0));\
            if (elements.length == 0) {\
                return 0;\
            }\
            var prev_len = WEBPLATFORM.rs_refs.length;\
            setValue($1, prev_len, 'i32');\
            Array.prototype.push.apply(WEBPLATFORM.rs_refs, elements);\
            return elements.length;\
        \0" };
        (start..(start+count)).map(|id| HtmlNode{ id: id, doc: self }).collect()
    }
}

pub struct LocalStorageInterface;

pub struct LocalStorageIterator {
    index: i32,
}

impl LocalStorageInterface {
    pub fn len(&self) -> i32 {
        js! { b"\
            return window.localStorage.length;\
        \0" }
    }

    pub fn clear(&self) {
        js! { b"\
            window.localStorage.clear();\
        \0" };
    }

    pub fn remove(&self, s: &str) {
        js! { (s) b"\
            window.localStorage.removeItem(UTF8ToString($0));\
        \0" };
    }

    pub fn set(&self, s: &str, v: &str) {
        js! { (s, v) b"\
            window.localStorage.setItem(UTF8ToString($0), UTF8ToString($1));\
        \0" };
    }

    pub fn get(&self, name: &str) -> Option<String> {
        let a = js! { (name) b"\
            var str = window.localStorage.getItem(UTF8ToString($0));\
            if (str == null) {\
                return -1;\
            }\
            return allocate(intArrayFromString(str), 'i8', ALLOC_STACK);\
        \0" };
        if a == -1 {
            None
        } else {
            Some(unsafe {
                str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
            })
        }
    }

    pub fn key(&self, index: i32) -> String {
        let a = js! { (index) b"\
            var key = window.localStorage.key($0);\
            return allocate(intArrayFromString(str), 'i8', ALLOC_STACK);\
        \0" };
        unsafe {
            str::from_utf8(CStr::from_ptr(a as *const libc::c_char).to_bytes()).unwrap().to_owned()
        }
    }
}

impl IntoIterator for LocalStorageInterface {
    type Item = String;
    type IntoIter = LocalStorageIterator;

    fn into_iter(self) -> LocalStorageIterator {
        LocalStorageIterator { index: 0 }
    }
}

impl Iterator for LocalStorageIterator {
    type Item = String;
    fn next(&mut self) -> Option<String> {
        if self.index >= LocalStorage.len() {
            None
        } else {
            LocalStorage.get(&LocalStorage.key(self.index))
        }
    }
}

#[allow(non_upper_case_globals)]
pub const LocalStorage: LocalStorageInterface = LocalStorageInterface;

pub fn init<'a>() -> Document<'a> {
    js! { b"\
        console.log('hi');\
        window.WEBPLATFORM || (window.WEBPLATFORM = {\
            rs_refs: [],\
        });\
        \"use strict\";var range;var NS_XHTML=\"http://www.w3.org/1999/xhtml\";var doc=typeof document===\"undefined\"?undefined:document;var testEl=doc?doc.body||doc.createElement(\"div\"):{};var actualHasAttributeNS;if(testEl.hasAttributeNS){actualHasAttributeNS=function(el,namespaceURI,name){return el.hasAttributeNS(namespaceURI,name)}}else if(testEl.hasAttribute){actualHasAttributeNS=function(el,namespaceURI,name){return el.hasAttribute(name)}}else{actualHasAttributeNS=function(el,namespaceURI,name){return el.getAttributeNode(namespaceURI,name)!=null}}var hasAttributeNS=actualHasAttributeNS;function toElement(str){if(!range&&doc.createRange){range=doc.createRange();range.selectNode(doc.body)}var fragment;if(range&&range.createContextualFragment){fragment=range.createContextualFragment(str)}else{fragment=doc.createElement(\"body\");fragment.innerHTML=str}return fragment.childNodes[0]}function compareNodeNames(fromEl,toEl){var fromNodeName=fromEl.nodeName;var toNodeName=toEl.nodeName;if(fromNodeName===toNodeName){return true}if(toEl.actualize&&fromNodeName.charCodeAt(0)<91&&toNodeName.charCodeAt(0)>90){return fromNodeName===toNodeName.toUpperCase()}else{return false}}function createElementNS(name,namespaceURI){return!namespaceURI||namespaceURI===NS_XHTML?doc.createElement(name):doc.createElementNS(namespaceURI,name)}function moveChildren(fromEl,toEl){var curChild=fromEl.firstChild;while(curChild){var nextChild=curChild.nextSibling;toEl.appendChild(curChild);curChild=nextChild}return toEl}function morphAttrs(fromNode,toNode){var attrs=toNode.attributes;var i;var attr;var attrName;var attrNamespaceURI;var attrValue;var fromValue;for(i=attrs.length-1;i>=0;--i){attr=attrs[i];attrName=attr.name;attrNamespaceURI=attr.namespaceURI;attrValue=attr.value;if(attrNamespaceURI){attrName=attr.localName||attrName;fromValue=fromNode.getAttributeNS(attrNamespaceURI,attrName);if(fromValue!==attrValue){fromNode.setAttributeNS(attrNamespaceURI,attrName,attrValue)}}else{fromValue=fromNode.getAttribute(attrName);if(fromValue!==attrValue){fromNode.setAttribute(attrName,attrValue)}}}attrs=fromNode.attributes;for(i=attrs.length-1;i>=0;--i){attr=attrs[i];if(attr.specified!==false){attrName=attr.name;attrNamespaceURI=attr.namespaceURI;if(attrNamespaceURI){attrName=attr.localName||attrName;if(!hasAttributeNS(toNode,attrNamespaceURI,attrName)){fromNode.removeAttributeNS(attrNamespaceURI,attrName)}}else{if(!hasAttributeNS(toNode,null,attrName)){fromNode.removeAttribute(attrName)}}}}}function syncBooleanAttrProp(fromEl,toEl,name){if(fromEl[name]!==toEl[name]){fromEl[name]=toEl[name];if(fromEl[name]){fromEl.setAttribute(name,\"\")}else{fromEl.removeAttribute(name,\"\")}}}var specialElHandlers={OPTION:function(fromEl,toEl){syncBooleanAttrProp(fromEl,toEl,\"selected\")},INPUT:function(fromEl,toEl){syncBooleanAttrProp(fromEl,toEl,\"checked\");syncBooleanAttrProp(fromEl,toEl,\"disabled\");if(fromEl.value!==toEl.value){fromEl.value=toEl.value}if(!hasAttributeNS(toEl,null,\"value\")){fromEl.removeAttribute(\"value\")}},TEXTAREA:function(fromEl,toEl){var newValue=toEl.value;if(fromEl.value!==newValue){fromEl.value=newValue}if(fromEl.firstChild){if(newValue===\"\"&&fromEl.firstChild.nodeValue===fromEl.placeholder){return}fromEl.firstChild.nodeValue=newValue}},SELECT:function(fromEl,toEl){if(!hasAttributeNS(toEl,null,\"multiple\")){var selectedIndex=-1;var i=0;var curChild=toEl.firstChild;while(curChild){var nodeName=curChild.nodeName;if(nodeName&&nodeName.toUpperCase()===\"OPTION\"){if(hasAttributeNS(curChild,null,\"selected\")){selectedIndex=i;break}i++}curChild=curChild.nextSibling}fromEl.selectedIndex=i}}};var ELEMENT_NODE=1;var TEXT_NODE=3;var COMMENT_NODE=8;function noop(){}function defaultGetNodeKey(node){return node.id}function morphdomFactory(morphAttrs){return function morphdom(fromNode,toNode,options){if(!options){options={}}if(typeof toNode===\"string\"){if(fromNode.nodeName===\"#document\"||fromNode.nodeName===\"HTML\"){var toNodeHtml=toNode;toNode=doc.createElement(\"html\");toNode.innerHTML=toNodeHtml}else{toNode=toElement(toNode)}}var getNodeKey=options.getNodeKey||defaultGetNodeKey;var onBeforeNodeAdded=options.onBeforeNodeAdded||noop;var onNodeAdded=options.onNodeAdded||noop;var onBeforeElUpdated=options.onBeforeElUpdated||noop;var onElUpdated=options.onElUpdated||noop;var onBeforeNodeDiscarded=options.onBeforeNodeDiscarded||noop;var onNodeDiscarded=options.onNodeDiscarded||noop;var onBeforeElChildrenUpdated=options.onBeforeElChildrenUpdated||noop;var childrenOnly=options.childrenOnly===true;var fromNodesLookup={};var keyedRemovalList;function addKeyedRemoval(key){if(keyedRemovalList){keyedRemovalList.push(key)}else{keyedRemovalList=[key]}}function walkDiscardedChildNodes(node,skipKeyedNodes){if(node.nodeType===ELEMENT_NODE){var curChild=node.firstChild;while(curChild){var key=undefined;if(skipKeyedNodes&&(key=getNodeKey(curChild))){addKeyedRemoval(key)}else{onNodeDiscarded(curChild);if(curChild.firstChild){walkDiscardedChildNodes(curChild,skipKeyedNodes)}}curChild=curChild.nextSibling}}}function removeNode(node,parentNode,skipKeyedNodes){if(onBeforeNodeDiscarded(node)===false){return}if(parentNode){parentNode.removeChild(node)}onNodeDiscarded(node);walkDiscardedChildNodes(node,skipKeyedNodes)}function indexTree(node){if(node.nodeType===ELEMENT_NODE){var curChild=node.firstChild;while(curChild){var key=getNodeKey(curChild);if(key){fromNodesLookup[key]=curChild}indexTree(curChild);curChild=curChild.nextSibling}}}indexTree(fromNode);function handleNodeAdded(el){onNodeAdded(el);var curChild=el.firstChild;while(curChild){var nextSibling=curChild.nextSibling;var key=getNodeKey(curChild);if(key){var unmatchedFromEl=fromNodesLookup[key];if(unmatchedFromEl&&compareNodeNames(curChild,unmatchedFromEl)){curChild.parentNode.replaceChild(unmatchedFromEl,curChild);morphEl(unmatchedFromEl,curChild)}}handleNodeAdded(curChild);curChild=nextSibling}}function morphEl(fromEl,toEl,childrenOnly){var toElKey=getNodeKey(toEl);var curFromNodeKey;if(toElKey){delete fromNodesLookup[toElKey]}if(toNode.isSameNode&&toNode.isSameNode(fromNode)){return}if(!childrenOnly){if(onBeforeElUpdated(fromEl,toEl)===false){return}morphAttrs(fromEl,toEl);onElUpdated(fromEl);if(onBeforeElChildrenUpdated(fromEl,toEl)===false){return}}if(fromEl.nodeName!==\"TEXTAREA\"){var curToNodeChild=toEl.firstChild;var curFromNodeChild=fromEl.firstChild;var curToNodeKey;var fromNextSibling;var toNextSibling;var matchingFromEl;outer:while(curToNodeChild){toNextSibling=curToNodeChild.nextSibling;curToNodeKey=getNodeKey(curToNodeChild);while(curFromNodeChild){fromNextSibling=curFromNodeChild.nextSibling;if(curToNodeChild.isSameNode&&curToNodeChild.isSameNode(curFromNodeChild)){curToNodeChild=toNextSibling;curFromNodeChild=fromNextSibling;continue outer}curFromNodeKey=getNodeKey(curFromNodeChild);var curFromNodeType=curFromNodeChild.nodeType;var isCompatible=undefined;if(curFromNodeType===curToNodeChild.nodeType){if(curFromNodeType===ELEMENT_NODE){if(curToNodeKey){if(curToNodeKey!==curFromNodeKey){if(matchingFromEl=fromNodesLookup[curToNodeKey]){if(curFromNodeChild.nextSibling===matchingFromEl){isCompatible=false}else{fromEl.insertBefore(matchingFromEl,curFromNodeChild);fromNextSibling=curFromNodeChild.nextSibling;if(curFromNodeKey){addKeyedRemoval(curFromNodeKey)}else{removeNode(curFromNodeChild,fromEl,true)}curFromNodeChild=matchingFromEl}}else{isCompatible=false}}}else if(curFromNodeKey){isCompatible=false}isCompatible=isCompatible!==false&&compareNodeNames(curFromNodeChild,curToNodeChild);if(isCompatible){morphEl(curFromNodeChild,curToNodeChild)}}else if(curFromNodeType===TEXT_NODE||curFromNodeType==COMMENT_NODE){isCompatible=true;curFromNodeChild.nodeValue=curToNodeChild.nodeValue}}if(isCompatible){curToNodeChild=toNextSibling;curFromNodeChild=fromNextSibling;continue outer}if(curFromNodeKey){addKeyedRemoval(curFromNodeKey)}else{removeNode(curFromNodeChild,fromEl,true)}curFromNodeChild=fromNextSibling}if(curToNodeKey&&(matchingFromEl=fromNodesLookup[curToNodeKey])&&compareNodeNames(matchingFromEl,curToNodeChild)){fromEl.appendChild(matchingFromEl);morphEl(matchingFromEl,curToNodeChild)}else{var onBeforeNodeAddedResult=onBeforeNodeAdded(curToNodeChild);if(onBeforeNodeAddedResult!==false){if(onBeforeNodeAddedResult){curToNodeChild=onBeforeNodeAddedResult}if(curToNodeChild.actualize){curToNodeChild=curToNodeChild.actualize(fromEl.ownerDocument||doc)}fromEl.appendChild(curToNodeChild);handleNodeAdded(curToNodeChild)}}curToNodeChild=toNextSibling;curFromNodeChild=fromNextSibling}while(curFromNodeChild){fromNextSibling=curFromNodeChild.nextSibling;if(curFromNodeKey=getNodeKey(curFromNodeChild)){addKeyedRemoval(curFromNodeKey)}else{removeNode(curFromNodeChild,fromEl,true)}curFromNodeChild=fromNextSibling}}var specialElHandler=specialElHandlers[fromEl.nodeName];if(specialElHandler){specialElHandler(fromEl,toEl)}}var morphedNode=fromNode;var morphedNodeType=morphedNode.nodeType;var toNodeType=toNode.nodeType;if(!childrenOnly){if(morphedNodeType===ELEMENT_NODE){if(toNodeType===ELEMENT_NODE){if(!compareNodeNames(fromNode,toNode)){onNodeDiscarded(fromNode);morphedNode=moveChildren(fromNode,createElementNS(toNode.nodeName,toNode.namespaceURI))}}else{morphedNode=toNode}}else if(morphedNodeType===TEXT_NODE||morphedNodeType===COMMENT_NODE){if(toNodeType===morphedNodeType){morphedNode.nodeValue=toNode.nodeValue;return morphedNode}else{morphedNode=toNode}}}if(morphedNode===toNode){onNodeDiscarded(fromNode)}else{morphEl(morphedNode,toNode,childrenOnly);if(keyedRemovalList){for(var i=0,len=keyedRemovalList.length;i<len;i++){var elToRemove=fromNodesLookup[keyedRemovalList[i]];if(elToRemove){removeNode(elToRemove,elToRemove.parentNode,false)}}}}if(!childrenOnly&&morphedNode!==fromNode&&fromNode.parentNode){if(morphedNode.actualize){morphedNode=morphedNode.actualize(fromNode.ownerDocument||doc)}fromNode.parentNode.replaceChild(morphedNode,fromNode)}return morphedNode}}window.morphdom=morphdomFactory(morphAttrs);\
        console.log('Loaded morphdom: '+(typeof window.morphdom=='function'));\
    \0" };
    Document {
        refs: Rc::new(RefCell::new(Vec::new())),
    }
}

extern fn leavemebe() {
    unsafe {
        emscripten_pause_main_loop();
    }
}

pub fn spin() {
    unsafe {
        emscripten_set_main_loop(leavemebe, 0, 1);

    }
}

#[no_mangle]
pub extern "C" fn syscall(a: i32) -> i32 {
    if a == 355 {
        return 55
    }
    return -1
}
