use scanner::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
enum State {
    StreamStart,
    ImplicitDocumentStart,
    DocumentStart,
    DocumentContent,
    DocumentEnd,
    BlockNode,
    // BlockNodeOrIndentlessSequence,
    // FlowNode,
    BlockSequenceFirstEntry,
    BlockSequenceEntry,
    IndentlessSequenceEntry,
    BlockMappingFirstKey,
    BlockMappingKey,
    BlockMappingValue,
    FlowSequenceFirstEntry,
    FlowSequenceEntry,
    FlowSequenceEntryMappingKey,
    FlowSequenceEntryMappingValue,
    FlowSequenceEntryMappingEnd,
    FlowMappingFirstKey,
    FlowMappingKey,
    FlowMappingValue,
    FlowMappingEmptyValue,
    End,
}

pub type AnchorId = usize;

/// `Event` is used with the low-level event base parsing API,
/// see `EventReceiver` trait.
#[derive(Clone, PartialEq, Debug, Eq)]
pub enum Event {
    /// Reserved for internal use
    Nothing,
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    /// Refer to an anchor ID
    Alias(AnchorId),
    /// Value, style, anchor_id, tag
    Scalar{value: String, style: TScalarStyle, anchor: Option<AnchorId>, tag: Option<TokenType>},
    /// Anchor ID
    SequenceStart(Option<AnchorId>),
    SequenceEnd,
    /// Anchor ID
    MappingStart(Option<AnchorId>),
    MappingEnd,
}

impl Event {
    fn empty_scalar() -> Event {
        // a null scalar
        Event::Scalar{value: "~".to_owned(), style: TScalarStyle::Plain, anchor: None, tag: None}
    }

    fn empty_scalar_with_anchor(anchor: Option<AnchorId>, tag: Option<TokenType>) -> Event {
        Event::Scalar{value: "".to_owned(), style: TScalarStyle::Plain, anchor, tag}
    }
}

#[derive(Debug)]
pub struct Parser<T> {
    scanner: Scanner<T>,
    states: Vec<State>,
    state: State,
    marks: Vec<Marker>,
    token: Option<Token>,
    current: Option<ParsedEventMarker>,
    anchors: HashMap<String, AnchorId>,
    next_anchor_id: AnchorId,
}

pub trait EventReceiver {
    fn on_event(&mut self, ev: Event);
}

pub trait MarkedEventReceiver {
    fn on_event(&mut self, ev: Event, _mark: Marker);
}

impl<R: EventReceiver> MarkedEventReceiver for R {
    fn on_event(&mut self, ev: Event, _mark: Marker) {
        self.on_event(ev)
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct ParsedEventMarker {
    event: Event, 
    mark: Marker,
}

impl ParsedEventMarker {
    fn new(event: Event, mark: Marker) -> Self {
        Self {
            event,
            mark,
        }
    }
}

pub type ParseResult = Result<ParsedEventMarker, ScanError>;

impl<T: Iterator<Item = char>> Parser<T> {
    pub fn new(src: T) -> Parser<T> {
        Parser {
            scanner: Scanner::new(src),
            states: Vec::new(),
            state: State::StreamStart,
            marks: Vec::new(),
            token: None,
            current: None,

            anchors: HashMap::new(),
            next_anchor_id: 1,
        }
    }

    pub fn peek(&mut self) -> Result<&ParsedEventMarker, ScanError> {
        match self.current {
            Some(ref x) => Ok(x),
            None => {
                self.current = Some(self.next()?);
                self.peek()
            }
        }
    }

    pub fn next(&mut self) -> ParseResult {
        match self.current {
            None => self.parse(),
            Some(_) => Ok(self.current.take().unwrap()),
        }
    }

    fn peek_token(&mut self) -> Result<&Token, ScanError> {
        match self.token {
            None => {
                self.token = Some(self.scan_next_token()?);
                Ok(self.token.as_ref().unwrap())
            }
            Some(ref tok) => Ok(tok),
        }
    }

    fn scan_next_token(&mut self) -> Result<Token, ScanError> {
        let token = self.scanner.next();
        match token {
            None => match self.scanner.get_error() {
                None => Err(ScanError::new(self.scanner.mark(), "unexpected eof")),
                Some(e) => Err(e),
            },
            Some(tok) => Ok(tok),
        }
    }

    fn fetch_token(&mut self) -> Token {
        self.token
            .take()
            .expect("fetch_token needs to be preceded by peek_token")
    }

    fn skip(&mut self) {
        self.token = None;
        //self.peek_token();
    }
    fn pop_state(&mut self) {
        self.state = self.states.pop().unwrap()
    }
    fn push_state(&mut self, state: State) {
        self.states.push(state);
    }

    fn parse(&mut self) -> ParseResult {
        if self.state == State::End {
            return Ok(ParsedEventMarker::new(Event::StreamEnd, self.scanner.mark()));
        }
        let event_marker = self.state_machine()?;
        // println!("EV {:?}", ev);
        Ok(event_marker)
    }

    pub fn load<R: MarkedEventReceiver>(
        &mut self,
        recv: &mut R,
        multi: bool,
    ) -> Result<(), ScanError> {
        if !self.scanner.stream_started() {
            let ParsedEventMarker{event, mark} = self.next()?;
            assert_eq!(event, Event::StreamStart);
            recv.on_event(event, mark);
        }

        if self.scanner.stream_ended() {
            // XXX has parsed?
            recv.on_event(Event::StreamEnd, self.scanner.mark());
            return Ok(());
        }
        loop {
            let ParsedEventMarker{event, mark} = self.next()?;
            if event == Event::StreamEnd {
                recv.on_event(event, mark);
                return Ok(());
            }
            // clear anchors before a new document
            self.anchors.clear();
            self.load_document(event, mark, recv)?;
            if !multi {
                break;
            }
        }
        Ok(())
    }

    fn load_document<R: MarkedEventReceiver>(
        &mut self,
        first_ev: Event,
        mark: Marker,
        recv: &mut R,
    ) -> Result<(), ScanError> {
        assert_eq!(first_ev, Event::DocumentStart);
        recv.on_event(first_ev, mark);

        let ParsedEventMarker{event, mark} = self.next()?;
        self.load_node(event, mark, recv)?;

        // DOCUMENT-END is expected.
        let ParsedEventMarker{event, mark} = self.next()?;
        assert_eq!(event, Event::DocumentEnd);
        recv.on_event(event, mark);

        Ok(())
    }

    fn load_node<R: MarkedEventReceiver>(
        &mut self,
        first_ev: Event,
        mark: Marker,
        recv: &mut R,
    ) -> Result<(), ScanError> {
        match first_ev {
            Event::Alias(..) | Event::Scalar{..} => {
                recv.on_event(first_ev, mark);
                Ok(())
            }
            Event::SequenceStart(_) => {
                recv.on_event(first_ev, mark);
                self.load_sequence(recv)
            }
            Event::MappingStart(_) => {
                recv.on_event(first_ev, mark);
                self.load_mapping(recv)
            }
            _ => {
                println!("UNREACHABLE EVENT: {:?}", first_ev);
                unreachable!();
            }
        }
    }

    fn load_mapping<R: MarkedEventReceiver>(&mut self, recv: &mut R) -> Result<(), ScanError> {
        let ParsedEventMarker{event: mut key_event, mark: mut key_mark} = self.next()?;
        while key_event != Event::MappingEnd {
            // key
            self.load_node(key_event, key_mark, recv)?;

            // value
            let ParsedEventMarker{event, mark} = self.next()?;
            self.load_node(event, mark, recv)?;

            // next event
            let ParsedEventMarker{event, mark} = self.next()?;
            key_event = event;
            key_mark = mark;
        }
        recv.on_event(key_event, key_mark);
        Ok(())
    }

    fn load_sequence<R: MarkedEventReceiver>(&mut self, recv: &mut R) -> Result<(), ScanError> {
        let ParsedEventMarker{mut event, mut mark} = self.next()?;
        while event != Event::SequenceEnd {
            self.load_node(event, mark, recv)?;

            // next event
            let ParsedEventMarker{event: next_event, mark: next_mark} = self.next()?;
            event = next_event;
            mark = next_mark;
        }
        recv.on_event(event, mark);
        Ok(())
    }

    fn state_machine(&mut self) -> ParseResult {
        // let next_tok = self.peek_token()?;
        // println!("cur_state {:?}, next tok: {:?}", self.state, next_tok);
        match self.state {
            State::StreamStart => self.stream_start(),

            State::ImplicitDocumentStart => self.document_start(true),
            State::DocumentStart => self.document_start(false),
            State::DocumentContent => self.document_content(),
            State::DocumentEnd => self.document_end(),

            State::BlockNode => self.parse_node(true, false),
            // State::BlockNodeOrIndentlessSequence => self.parse_node(true, true),
            // State::FlowNode => self.parse_node(false, false),
            State::BlockMappingFirstKey => self.block_mapping_key(true),
            State::BlockMappingKey => self.block_mapping_key(false),
            State::BlockMappingValue => self.block_mapping_value(),

            State::BlockSequenceFirstEntry => self.block_sequence_entry(true),
            State::BlockSequenceEntry => self.block_sequence_entry(false),

            State::FlowSequenceFirstEntry => self.flow_sequence_entry(true),
            State::FlowSequenceEntry => self.flow_sequence_entry(false),

            State::FlowMappingFirstKey => self.flow_mapping_key(true),
            State::FlowMappingKey => self.flow_mapping_key(false),
            State::FlowMappingValue => self.flow_mapping_value(false),

            State::IndentlessSequenceEntry => self.indentless_sequence_entry(),

            State::FlowSequenceEntryMappingKey => self.flow_sequence_entry_mapping_key(),
            State::FlowSequenceEntryMappingValue => self.flow_sequence_entry_mapping_value(),
            State::FlowSequenceEntryMappingEnd => self.flow_sequence_entry_mapping_end(),
            State::FlowMappingEmptyValue => self.flow_mapping_value(true),

            /* impossible */
            State::End => unreachable!(),
        }
    }

    fn stream_start(&mut self) -> ParseResult {
        match *self.peek_token()? {
            Token{tokentype: TokenType::StreamStart(_), mark} => {
                self.state = State::ImplicitDocumentStart;
                self.skip();
                Ok(ParsedEventMarker::new(Event::StreamStart, mark))
            }
            Token{mark, ..} => Err(ScanError::new(mark, "did not find expected <stream-start>")),
        }
    }

    fn document_start(&mut self, implicit: bool) -> ParseResult {
        if !implicit {
            while let TokenType::DocumentEnd = self.peek_token()?.tokentype {
                self.skip();
            }
        }

        match *self.peek_token()? {
            Token{tokentype: TokenType::StreamEnd, mark} => {
                self.state = State::End;
                self.skip();
                Ok(ParsedEventMarker::new(Event::StreamEnd, mark))
            }
            Token{tokentype: TokenType::VersionDirective(..), ..}
            | Token{tokentype: TokenType::TagDirective(..), ..}
            | Token{tokentype: TokenType::DocumentStart, ..} => {
                // explicit document
                self._explict_document_start()
            }
            Token{mark, ..} if implicit => {
                self.parser_process_directives()?;
                self.push_state(State::DocumentEnd);
                self.state = State::BlockNode;
                Ok(ParsedEventMarker::new(Event::DocumentStart, mark))
            }
            _ => {
                // explicit document
                self._explict_document_start()
            }
        }
    }

    fn parser_process_directives(&mut self) -> Result<(), ScanError> {
        loop {
            match self.peek_token()?.tokentype {
                TokenType::VersionDirective(_, _) => {
                    // XXX parsing with warning according to spec
                    //if major != 1 || minor > 2 {
                    //    return Err(ScanError::new(tok.0,
                    //        "found incompatible YAML document"));
                    //}
                }
                TokenType::TagDirective(..) => {
                    // TODO add tag directive
                }
                _ => break,
            }
            self.skip();
        }
        // TODO tag directive
        Ok(())
    }

    fn _explict_document_start(&mut self) -> ParseResult {
        self.parser_process_directives()?;
        match *self.peek_token()? {
            Token{tokentype: TokenType::DocumentStart, mark} => {
                self.push_state(State::DocumentEnd);
                self.state = State::DocumentContent;
                self.skip();
                Ok(ParsedEventMarker::new(Event::DocumentStart, mark))
            }
            Token{mark, ..} => Err(ScanError::new(
                mark,
                "did not find expected <document start>",
            )),
        }
    }

    fn document_content(&mut self) -> ParseResult {
        match *self.peek_token()? {
            Token{tokentype: TokenType::VersionDirective(..), mark}
            | Token{tokentype: TokenType::TagDirective(..), mark}
            | Token{tokentype: TokenType::DocumentStart, mark}
            | Token{tokentype: TokenType::DocumentEnd, mark}
            | Token{tokentype: TokenType::StreamEnd, mark} => {
                self.pop_state();
                // empty scalar
                Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
            }
            _ => self.parse_node(true, false),
        }
    }

    fn document_end(&mut self) -> ParseResult {
        let mut _implicit = true;
        let marker: Marker = match *self.peek_token()? {
            Token{tokentype: TokenType::DocumentEnd, mark} => {
                self.skip();
                _implicit = false;
                mark
            }
            Token{mark, ..} => mark,
        };

        // TODO tag handling
        self.state = State::DocumentStart;
        Ok(ParsedEventMarker::new(Event::DocumentEnd, marker))
    }

    fn register_anchor(&mut self, name: String, _: &Marker) -> Result<AnchorId, ScanError> {
        // anchors can be overrided/reused
        // if self.anchors.contains_key(name) {
        //     return Err(ScanError::new(*mark,
        //         "while parsing anchor, found duplicated anchor"));
        // }
        let new_id = self.next_anchor_id;
        self.next_anchor_id += 1;
        self.anchors.insert(name, new_id);
        Ok(new_id)
    }

    fn parse_node(&mut self, block: bool, indentless_sequence: bool) -> ParseResult {
        let mut anchor = None;
        let mut tag = None;
        match *self.peek_token()? {
            Token{tokentype: TokenType::Alias(_), ..} => {
                self.pop_state();
                if let Token{tokentype: TokenType::Alias(name), mark} = self.fetch_token() {
                    match self.anchors.get(&name) {
                        None => {
                            return Err(ScanError::new(
                                mark,
                                "while parsing node, found unknown anchor",
                            ))
                        }
                        Some(id) => return Ok(ParsedEventMarker::new(Event::Alias(*id), mark)),
                    }
                } else {
                    unreachable!()
                }
            }
            Token{tokentype: TokenType::Anchor(_), ..} => {
                if let Token{mark, tokentype: TokenType::Anchor(name)} = self.fetch_token() {
                    anchor = Some(self.register_anchor(name, &mark)?);
                    if let TokenType::Tag(..) = self.peek_token()?.tokentype {
                        if let tg @ TokenType::Tag(..) = self.fetch_token().tokentype {
                            tag = Some(tg);
                        } else {
                            unreachable!()
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            Token{tokentype: TokenType::Tag(..), ..} => {
                if let tg @ TokenType::Tag(..) = self.fetch_token().tokentype {
                    tag = Some(tg);
                    if let TokenType::Anchor(_) = self.peek_token()?.tokentype {
                        if let Token{tokentype: TokenType::Anchor(name), mark} = self.fetch_token() {
                            anchor = Some(self.register_anchor(name, &mark)?);
                        } else {
                            unreachable!()
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            _ => {}
        }
        match *self.peek_token()? {
            Token{tokentype: TokenType::BlockEntry, mark} if indentless_sequence => {
                self.state = State::IndentlessSequenceEntry;
                Ok(ParsedEventMarker::new(Event::SequenceStart(anchor), mark))
            }
            Token{tokentype: TokenType::Scalar(..), ..} => {
                self.pop_state();
                if let Token{tokentype: TokenType::Scalar(style, value), mark} = self.fetch_token() {
                    Ok(ParsedEventMarker::new(Event::Scalar{value, style, anchor, tag}, mark))
                } else {
                    unreachable!()
                }
            }
            Token{tokentype: TokenType::FlowSequenceStart, mark} => {
                self.state = State::FlowSequenceFirstEntry;
                Ok(ParsedEventMarker::new(Event::SequenceStart(anchor), mark))
            }
            Token{tokentype: TokenType::FlowMappingStart, mark} => {
                self.state = State::FlowMappingFirstKey;
                Ok(ParsedEventMarker::new(Event::MappingStart(anchor), mark))
            }
            Token{tokentype: TokenType::BlockSequenceStart, mark} if block => {
                self.state = State::BlockSequenceFirstEntry;
                Ok(ParsedEventMarker::new(Event::SequenceStart(anchor), mark))
            }
            Token{tokentype: TokenType::BlockMappingStart, mark} if block => {
                self.state = State::BlockMappingFirstKey;
                Ok(ParsedEventMarker::new(Event::MappingStart(anchor), mark))
            }
            // ex 7.2, an empty scalar can follow a secondary tag
            Token{mark, ..} if tag.is_some() || anchor.is_some() => {
                self.pop_state();
                Ok(ParsedEventMarker::new(Event::empty_scalar_with_anchor(anchor, tag), mark))
            }
            Token{mark, ..} => Err(ScanError::new(
                mark,
                "while parsing a node, did not find expected node content",
            )),
        }
    }

    fn block_mapping_key(&mut self, first: bool) -> ParseResult {
        // skip BlockMappingStart
        if first {
            let _ = self.peek_token()?;
            //self.marks.push(tok.0);
            self.skip();
        }
        match *self.peek_token()? {
            Token{tokentype: TokenType::Key, ..} => {
                self.skip();
                match *self.peek_token()? {
                    Token{tokentype: TokenType::Key, mark}
                    | Token{tokentype: TokenType::Value, mark}
                    | Token{tokentype: TokenType::BlockEnd, mark} => {
                        self.state = State::BlockMappingValue;
                        // empty scalar
                        Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
                    }
                    _ => {
                        self.push_state(State::BlockMappingValue);
                        self.parse_node(true, true)
                    }
                }
            }
            // XXX(chenyh): libyaml failed to parse spec 1.2, ex8.18
            Token{tokentype: TokenType::Value, mark} => {
                self.state = State::BlockMappingValue;
                Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
            }
            Token{tokentype: TokenType::BlockEnd, mark} => {
                self.pop_state();
                self.skip();
                Ok(ParsedEventMarker::new(Event::MappingEnd, mark))
            }
            Token{mark, ..} => Err(ScanError::new(
                mark,
                "while parsing a block mapping, did not find expected key",
            )),
        }
    }

    fn block_mapping_value(&mut self) -> ParseResult {
        match *self.peek_token()? {
            Token{tokentype: TokenType::Value, ..} => {
                self.skip();
                match *self.peek_token()? {
                    Token{tokentype: TokenType::Key, mark}
                    | Token{tokentype: TokenType::Value, mark}
                    | Token{tokentype: TokenType::BlockEnd, mark} => {
                        self.state = State::BlockMappingKey;
                        // empty scalar
                        Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
                    }
                    _ => {
                        self.push_state(State::BlockMappingKey);
                        self.parse_node(true, true)
                    }
                }
            }
            Token{mark, ..} => {
                self.state = State::BlockMappingKey;
                // empty scalar
                Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
            }
        }
    }

    fn flow_mapping_key(&mut self, first: bool) -> ParseResult {
        if first {
            let _ = self.peek_token()?;
            self.skip();
        }
        let marker: Marker =
            {
                match *self.peek_token()? {
                    Token{tokentype: TokenType::FlowMappingEnd, mark} => mark,
                    Token{mark, ..} => {
                        if !first {
                            match *self.peek_token()? {
                            Token{tokentype: TokenType::FlowEntry, ..} => self.skip(),
                            Token{mark, ..} => return Err(ScanError::new(mark,
                                "while parsing a flow mapping, did not find expected ',' or '}'"))
                        }
                        }

                        match *self.peek_token()? {
                            Token{tokentype: TokenType::Key, ..} => {
                                self.skip();
                                match *self.peek_token()? {
                                    Token{tokentype: TokenType::Value, mark}
                                    | Token{tokentype: TokenType::FlowEntry, mark}
                                    | Token{tokentype: TokenType::FlowMappingEnd, mark} => {
                                        self.state = State::FlowMappingValue;
                                        return Ok(ParsedEventMarker::new(Event::empty_scalar(), mark));
                                    }
                                    _ => {
                                        self.push_state(State::FlowMappingValue);
                                        return self.parse_node(false, false);
                                    }
                                }
                            }
                            Token{tokentype: TokenType::Value, mark} => {
                                self.state = State::FlowMappingValue;
                                return Ok(ParsedEventMarker::new(Event::empty_scalar(), mark));
                            }
                            Token{tokentype: TokenType::FlowMappingEnd, ..} => (),
                            _ => {
                                self.push_state(State::FlowMappingEmptyValue);
                                return self.parse_node(false, false);
                            }
                        }

                        mark
                    }
                }
            };

        self.pop_state();
        self.skip();
        Ok(ParsedEventMarker::new(Event::MappingEnd, marker))
    }

    fn flow_mapping_value(&mut self, empty: bool) -> ParseResult {
        let mark: Marker = {
            if empty {
                let Token{mark, ..} = *self.peek_token()?;
                self.state = State::FlowMappingKey;
                return Ok(ParsedEventMarker::new(Event::empty_scalar(), mark));
            } else {
                match *self.peek_token()? {
                    Token{tokentype: TokenType::Value, mark} => {
                        self.skip();
                        match self.peek_token()?.tokentype {
                            TokenType::FlowEntry | TokenType::FlowMappingEnd => {}
                            _ => {
                                self.push_state(State::FlowMappingKey);
                                return self.parse_node(false, false);
                            }
                        }
                        mark
                    }
                    Token{mark, ..} => mark,
                }
            }
        };

        self.state = State::FlowMappingKey;
        Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
    }

    fn flow_sequence_entry(&mut self, first: bool) -> ParseResult {
        // skip FlowMappingStart
        if first {
            let _ = self.peek_token()?;
            //self.marks.push(tok.0);
            self.skip();
        }
        match *self.peek_token()? {
            Token{tokentype: TokenType::FlowSequenceEnd, mark} => {
                self.pop_state();
                self.skip();
                return Ok(ParsedEventMarker::new(Event::SequenceEnd, mark));
            }
            Token{tokentype: TokenType::FlowEntry, ..} if !first => {
                self.skip();
            }
            Token{mark, ..} if !first => {
                return Err(ScanError::new(
                    mark,
                    "while parsing a flow sequence, expectd ',' or ']'",
                ));
            }
            _ => { /* next */ }
        }
        match *self.peek_token()? {
            Token{tokentype: TokenType::FlowSequenceEnd, mark} => {
                self.pop_state();
                self.skip();
                Ok(ParsedEventMarker::new(Event::SequenceEnd, mark))
            }
            Token{tokentype: TokenType::Key, mark} => {
                self.state = State::FlowSequenceEntryMappingKey;
                self.skip();
                Ok(ParsedEventMarker::new(Event::MappingStart(None), mark))
            }
            _ => {
                self.push_state(State::FlowSequenceEntry);
                self.parse_node(false, false)
            }
        }
    }

    fn indentless_sequence_entry(&mut self) -> ParseResult {
        match *self.peek_token()? {
            Token{tokentype: TokenType::BlockEntry, ..} => (),
            Token{mark, ..} => {
                self.pop_state();
                return Ok(ParsedEventMarker::new(Event::SequenceEnd, mark));
            }
        }
        self.skip();
        match *self.peek_token()? {
            Token{tokentype: TokenType::BlockEntry, mark}
            | Token{tokentype: TokenType::Key, mark}
            | Token{tokentype: TokenType::Value, mark}
            | Token{tokentype: TokenType::BlockEnd, mark} => {
                self.state = State::IndentlessSequenceEntry;
                Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
            }
            _ => {
                self.push_state(State::IndentlessSequenceEntry);
                self.parse_node(true, false)
            }
        }
    }

    fn block_sequence_entry(&mut self, first: bool) -> ParseResult {
        // BLOCK-SEQUENCE-START
        if first {
            let _ = self.peek_token()?;
            //self.marks.push(tok.0);
            self.skip();
        }
        match *self.peek_token()? {
            Token{tokentype: TokenType::BlockEnd, mark} => {
                self.pop_state();
                self.skip();
                Ok(ParsedEventMarker::new(Event::SequenceEnd, mark))
            }
            Token{tokentype: TokenType::BlockEntry, ..} => {
                self.skip();
                match *self.peek_token()? {
                    Token{tokentype: TokenType::BlockEntry, mark} | Token{tokentype: TokenType::BlockEnd, mark} => {
                        self.state = State::BlockSequenceEntry;
                        Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
                    }
                    _ => {
                        self.push_state(State::BlockSequenceEntry);
                        self.parse_node(true, false)
                    }
                }
            }
            Token{mark, ..} => Err(ScanError::new(
                mark,
                "while parsing a block collection, did not find expected '-' indicator",
            )),
        }
    }

    fn flow_sequence_entry_mapping_key(&mut self) -> ParseResult {
        match *self.peek_token()? {
            Token{tokentype: TokenType::Value, mark}
            | Token{tokentype: TokenType::FlowEntry, mark}
            | Token{tokentype: TokenType::FlowSequenceEnd, mark} => {
                self.skip();
                self.state = State::FlowSequenceEntryMappingValue;
                Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
            }
            _ => {
                self.push_state(State::FlowSequenceEntryMappingValue);
                self.parse_node(false, false)
            }
        }
    }

    fn flow_sequence_entry_mapping_value(&mut self) -> ParseResult {
        match *self.peek_token()? {
            Token{tokentype: TokenType::Value, ..} => {
                self.skip();
                self.state = State::FlowSequenceEntryMappingValue;
                match *self.peek_token()? {
                    Token{tokentype: TokenType::FlowEntry, mark} | Token{tokentype: TokenType::FlowSequenceEnd, mark} => {
                        self.state = State::FlowSequenceEntryMappingEnd;
                        Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
                    }
                    _ => {
                        self.push_state(State::FlowSequenceEntryMappingEnd);
                        self.parse_node(false, false)
                    }
                }
            }
            Token{mark, ..} => {
                self.state = State::FlowSequenceEntryMappingEnd;
                Ok(ParsedEventMarker::new(Event::empty_scalar(), mark))
            }
        }
    }

    fn flow_sequence_entry_mapping_end(&mut self) -> ParseResult {
        self.state = State::FlowSequenceEntry;
        Ok(ParsedEventMarker::new(Event::MappingEnd, self.scanner.mark()))
    }
}

#[cfg(test)]
mod test {
    use super::{Event, Parser};

    #[test]
    fn test_peek_eq_parse() {
        let s = "
a0 bb: val
a1: &x
    b1: 4
    b2: d
a2: 4
a3: [1, 2, 3]
a4:
    - [a1, a2]
    - 2
a5: *x
";
        let mut p = Parser::new(s.chars());
        while {
            let event_peek = p.peek().unwrap().clone();
            let event = p.next().unwrap();
            assert_eq!(event, event_peek);
            event.event != Event::StreamEnd
        } {}
    }
}
