use crate::*;

impl Mechanism {
    /// Delete all source occurrences of a pair/point name during structure
    /// entry. IDs are reassigned in retained source order; derived groups are
    /// discarded so callers rebuild them before further calculations.
    pub fn delete_pairs_named(&mut self, name: Name) -> Result<usize, CoreError> {
        let deleted = self
            .pairs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.name == name)
            .map(|(i, _)| PairId(i))
            .collect::<Vec<_>>();
        if deleted.is_empty() {
            return Err(CoreError::InvalidStructure("pair or point name not found"));
        }
        let mut candidate = self.clone();
        candidate.groups.clear();
        for l in &mut candidate.links {
            let mut survivors = l
                .pairs
                .into_iter()
                .flatten()
                .chain(l.extra_pairs.iter().copied())
                .filter(|p| !deleted.contains(p));
            l.pairs = [survivors.next(), survivors.next()];
            l.extra_pairs = survivors.collect();
        }
        candidate.points.retain(|p| {
            !(deleted.contains(&p.pair)
                || p.source == AttachmentSource::ExtraPair
                    && candidate.links[p.owner.0].pairs.contains(&Some(p.pair)))
        });
        let retained = candidate
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| l.pairs[0].is_some())
            .map(|(i, _)| LinkId(i))
            .collect::<Vec<_>>();
        for p in &candidate.points {
            if !retained.contains(&p.owner) {
                return Err(CoreError::UndefinedPosition {
                    group: None,
                    reason: "point refers to a deleted link",
                });
            }
        }
        let mut link_map = vec![None; candidate.links.len()];
        for (n, id) in retained.iter().enumerate() {
            link_map[id.0] = Some(LinkId(n));
        }
        let mut pair_map = vec![None; candidate.pairs.len()];
        let mut pairs = Vec::new();
        for (old, p) in candidate.pairs.iter().enumerate() {
            if !deleted.contains(&PairId(old)) {
                let mut p = p.clone();
                for l in &mut p.links {
                    *l = link_map[l.0].ok_or(CoreError::UndefinedPosition {
                        group: None,
                        reason: "pair refers to a deleted link",
                    })?;
                }
                pair_map[old] = Some(PairId(pairs.len()));
                pairs.push(p);
            }
        }
        let old_points = self.points.clone();
        for p in &mut candidate.points {
            p.owner = link_map[p.owner.0].unwrap();
            p.pair = pair_map[p.pair.0].unwrap();
        }
        let mut links = Vec::new();
        for id in retained {
            let mut l = candidate.links[id.0].clone();
            l.pairs = l.pairs.map(|p| p.and_then(|p| pair_map[p.0]));
            l.extra_pairs = l
                .extra_pairs
                .into_iter()
                .filter_map(|p| pair_map[p.0])
                .collect();
            if let Some(mass) = &mut l.mass
                && let Some(center) = mass.center
            {
                let old = old_points
                    .get(center.0)
                    .ok_or(CoreError::MissingPoint(center))?;
                if deleted.contains(&old.pair) {
                    l.mass = None;
                } else {
                    mass.center = candidate
                        .points
                        .iter()
                        .position(|p| {
                            p.owner == link_map[old.owner.0].unwrap()
                                && p.pair == pair_map[old.pair.0].unwrap()
                        })
                        .map(PointId);
                }
            }
            links.push(l);
        }
        candidate.ground = candidate.ground.and_then(|p| link_map[p.0]);
        candidate.inputs = candidate
            .inputs
            .into_iter()
            .filter_map(|p| link_map[p.0])
            .collect();
        candidate.links = links;
        candidate.pairs = pairs;
        *self = candidate;
        Ok(deleted.len())
    }
    pub fn add_link(&mut self, name: Name) -> LinkId {
        if let Some(id) = self.link_by_name(name) {
            return id;
        }
        let id = LinkId(self.links.len());
        self.links.push(Link::new(name));
        id
    }
    pub fn add_pair(
        &mut self,
        kind: PairKind,
        name: Name,
        links: [LinkId; 2],
    ) -> Result<PairId, CoreError> {
        if links[0] == links[1] {
            return Err(CoreError::InvalidStructure(
                "a pair must connect different links",
            ));
        }
        for id in links {
            if self.link(id).is_none() {
                return Err(CoreError::MissingLink(id));
            }
        }
        if self
            .pairs
            .iter()
            .any(|p| p.name == name && p.links == links && p.kind == kind)
        {
            return Err(CoreError::InvalidStructure("duplicate pair entry"));
        }
        let id = PairId(self.pairs.len());
        self.pairs.push(Pair {
            kind,
            name,
            links,
            state: PairState::default(),
        });
        for owner in links {
            let link = &mut self.links[owner.0];
            if let Some(slot) = link.pairs.iter_mut().find(|p| p.is_none()) {
                *slot = Some(id);
            } else {
                link.extra_pairs.push(id);
                self.points.push(AttachedPoint {
                    owner,
                    pair: id,
                    source: AttachmentSource::ExtraPair,
                    geometry: LinkGeometry::default(),
                });
            }
        }
        Ok(id)
    }
    pub fn add_point(
        &mut self,
        owner: LinkId,
        kind: PairKind,
        name: Name,
    ) -> Result<PointId, CoreError> {
        if self.link(owner).is_none() {
            return Err(CoreError::MissingLink(owner));
        }
        if matches!(kind, PairKind::Revolute | PairKind::Prismatic) {
            return Err(CoreError::InvalidStructure(
                "additional points require T, M, or F kind",
            ));
        }
        let pair = PairId(self.pairs.len());
        self.pairs.push(Pair {
            kind,
            name,
            links: [owner, owner],
            state: PairState::default(),
        });
        let id = PointId(self.points.len());
        self.points.push(AttachedPoint {
            owner,
            pair,
            source: AttachmentSource::AdditionalPoint,
            geometry: LinkGeometry::default(),
        });
        Ok(id)
    }
    /// Preserve arena identities while setting the source ground/input order.
    pub fn set_ground_and_inputs(
        &mut self,
        ground: LinkId,
        inputs: Vec<LinkId>,
    ) -> Result<(), CoreError> {
        if self.link(ground).is_none() {
            return Err(CoreError::MissingLink(ground));
        }
        if inputs.is_empty() || self.formal_mobility() != inputs.len() as isize {
            return Err(CoreError::InvalidStructure(
                "input count must equal positive formal mobility",
            ));
        }
        for (i, id) in inputs.iter().enumerate() {
            if self.link(*id).is_none() {
                return Err(CoreError::MissingLink(*id));
            }
            if *id == ground || inputs[..i].contains(id) {
                return Err(CoreError::InvalidStructure(
                    "ground/input links must be distinct",
                ));
            }
        }
        let ground_link = &self.links[ground.0];
        let known = ground_link
            .pairs
            .into_iter()
            .flatten()
            .chain(ground_link.extra_pairs.iter().copied())
            .map(|p| self.pairs[p.0].name)
            .collect::<Vec<_>>();
        for id in &inputs {
            let link = &self.links[id.0];
            let p1 = link.pairs[0].ok_or(CoreError::InvalidStructure("input first pair absent"))?;
            let p2 =
                link.pairs[1].ok_or(CoreError::InvalidStructure("input second pair absent"))?;
            if !known.contains(&self.pairs[p1.0].name) {
                if known.contains(&self.pairs[p2.0].name) {
                    self.links[id.0].pairs.swap(0, 1);
                } else if link
                    .extra_pairs
                    .iter()
                    .any(|p| known.contains(&self.pairs[p.0].name))
                {
                    return Err(CoreError::UndefinedPosition {
                        group: None,
                        reason: "legacy input selection aborts after promoting an auxiliary ground pair",
                    });
                } else {
                    return Err(CoreError::InvalidStructure(
                        "input link must connect to ground",
                    ));
                }
            }
        }
        self.ground = Some(ground);
        self.inputs = inputs;
        Ok(())
    }
    fn append_attached_groups(
        &mut self,
        owner: LinkId,
        include_points: bool,
    ) -> Result<(), CoreError> {
        for source in [
            AttachmentSource::ExtraPair,
            AttachmentSource::AdditionalPoint,
        ] {
            if source == AttachmentSource::AdditionalPoint && !include_points {
                continue;
            }
            for (i, p) in self
                .points
                .iter()
                .enumerate()
                .filter(|(_, p)| p.owner == owner && p.source == source)
            {
                let pair = self.pair(p.pair).ok_or(CoreError::MissingPair(p.pair))?;
                let mut group = Group::new(GroupKind::Attached, owner);
                group.link_b = Some(owner);
                group.more = Some(p.pair);
                group.attached_point = Some(PointId(i));
                group.assembly = if matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic) {
                    b' '
                } else {
                    pair.kind.code()
                };
                self.groups.push(group);
            }
        }
        Ok(())
    }
    pub fn prepare_initial_groups(&mut self) -> Result<(), CoreError> {
        self.groups.clear();
        let ground = self
            .ground
            .ok_or(CoreError::InvalidStructure("ground not selected"))?;
        let link = self.link(ground).ok_or(CoreError::MissingLink(ground))?;
        let mut group = Group::new(GroupKind::Ground, ground);
        group.assembly = b'T';
        group.pair_x = link.pairs[0];
        group.pair_y = link.pairs[1];
        self.groups.push(group);
        self.append_attached_groups(ground, false)?;
        for id in self.inputs.clone() {
            let link = self.link(id).ok_or(CoreError::MissingLink(id))?;
            let mut group = Group::new(GroupKind::Input, id);
            group.assembly = b'N';
            group.pair_x = link.pairs[0];
            group.more = link.pairs[1];
            if group.pair_x.is_none() || group.more.is_none() {
                return Err(CoreError::InvalidStructure(
                    "input link needs two incident pairs",
                ));
            }
            self.groups.push(group);
            self.append_attached_groups(id, true)?;
        }
        self.classify_groups()
    }
    pub fn append_dyad(&mut self, a: LinkId, b: LinkId) -> Result<GroupId, CoreError> {
        if a == b
            || Some(a) == self.ground
            || Some(b) == self.ground
            || self.inputs.contains(&a)
            || self.inputs.contains(&b)
        {
            return Err(CoreError::InvalidStructure(
                "dyad requires distinct non-ground non-input links",
            ));
        }
        for id in [a, b] {
            if self.link(id).is_none() {
                return Err(CoreError::MissingLink(id));
            }
            if self
                .groups
                .iter()
                .any(|g| g.kind == GroupKind::Dyad && (g.link_a == id || g.link_b == Some(id)))
            {
                return Err(CoreError::InvalidStructure(
                    "link already belongs to a dyad",
                ));
            }
        }
        let incidents = |id: LinkId| {
            let l = &self.links[id.0];
            l.pairs
                .into_iter()
                .flatten()
                .chain(l.extra_pairs.iter().copied())
                .collect::<Vec<_>>()
        };
        let aa = incidents(a);
        let bb = incidents(b);
        let mut shared = aa
            .iter()
            .filter(|x| {
                bb.iter()
                    .any(|y| self.pairs[x.0].name == self.pairs[y.0].name)
            })
            .copied()
            .collect::<Vec<_>>();
        shared.dedup_by_key(|p| self.pairs[p.0].name);
        if shared.len() != 1 {
            return Err(CoreError::InvalidStructure(
                "dyad must share exactly one pair name",
            ));
        }
        let shared_name = self.pairs[shared[0].0].name;
        // U08 OrderSharedPair: swap the actual pointer slots, retaining each
        // auxiliary entry's place. Promoting an extra pair demotes the previous
        // primary pointer into that exact auxiliary entry.
        for owner in [a, b] {
            let l = &self.links[owner.0];
            let p1 = l.pairs[0].ok_or(CoreError::InvalidStructure("dyad first pair absent"))?;
            let p2 = l.pairs[1].ok_or(CoreError::InvalidStructure("dyad second pair absent"))?;
            if self.pairs[p1.0].name == shared_name {
                self.links[owner.0].pairs.swap(0, 1);
            } else if self.pairs[p2.0].name != shared_name {
                let index = l
                    .extra_pairs
                    .iter()
                    .position(|p| self.pairs[p.0].name == shared_name)
                    .ok_or(CoreError::InvalidStructure("shared dyad pair absent"))?;
                self.swap_extra_pair(owner, 1, index)?;
            }
        }
        for owner in [a, b] {
            let l = &self.links[owner.0];
            let known = |p: PairId| {
                self.groups.iter().any(|g| {
                    [g.pair_x, g.pair_y, g.more]
                        .into_iter()
                        .flatten()
                        .any(|q| self.pairs[q.0].name == self.pairs[p.0].name)
                })
            };
            let first =
                l.pairs[0].ok_or(CoreError::InvalidStructure("dyad first outer pair absent"))?;
            if !known(first) {
                let index = l.extra_pairs.iter().position(|p| known(*p)).ok_or(
                    CoreError::InvalidStructure("dyad outer pair is not yet known"),
                )?;
                self.swap_extra_pair(owner, 0, index)?;
            }
        }
        let id = GroupId(self.groups.len());
        let mut g = Group::new(GroupKind::Dyad, a);
        g.assembly = b'I';
        g.link_b = Some(b);
        g.pair_x = self.links[a.0].pairs[0];
        g.pair_y = self.links[b.0].pairs[0];
        g.more = self.links[b.0].pairs[1];
        self.groups.push(g);
        self.append_attached_groups(a, true)?;
        self.append_attached_groups(b, true)?;
        self.classify_groups()?;
        Ok(id)
    }
    fn swap_extra_pair(
        &mut self,
        owner: LinkId,
        slot: usize,
        index: usize,
    ) -> Result<(), CoreError> {
        let primary = self.links[owner.0].pairs[slot]
            .ok_or(CoreError::InvalidStructure("primary pair absent in swap"))?;
        let extra = self.links[owner.0].extra_pairs[index];
        self.links[owner.0].pairs[slot] = Some(extra);
        self.links[owner.0].extra_pairs[index] = primary;
        let entry = self
            .points
            .iter_mut()
            .find(|p| {
                p.owner == owner && p.source == AttachmentSource::ExtraPair && p.pair == extra
            })
            .ok_or(CoreError::InvalidStructure(
                "auxiliary entry absent in swap",
            ))?;
        entry.pair = primary;
        Ok(())
    }
    pub fn classify_link(&self, id: LinkId) -> Result<u8, CoreError> {
        let link = self.link(id).ok_or(CoreError::MissingLink(id))?;
        let mut n = 0;
        for p in link.pairs.into_iter().flatten() {
            if self.pair(p).ok_or(CoreError::MissingPair(p))?.kind == PairKind::Prismatic {
                n += 1;
            }
        }
        Ok(b'0' + n)
    }
    pub fn classify_groups(&mut self) -> Result<(), CoreError> {
        for index in 0..self.groups.len() {
            let g = &self.groups[index];
            let p = |id: Option<PairId>| -> Result<bool, CoreError> {
                let id = id.ok_or(CoreError::InvalidStructure("group pair absent"))?;
                Ok(self.pair(id).ok_or(CoreError::MissingPair(id))?.kind == PairKind::Prismatic)
            };
            let t = match g.kind {
                GroupKind::Ground => b' ',
                GroupKind::Input => b'0' + u8::from(p(g.pair_x)?) + 2 * u8::from(p(g.more)?),
                GroupKind::Attached => {
                    let mut t = self.classify_link(
                        g.link_b
                            .ok_or(CoreError::InvalidStructure("attached owner absent"))?,
                    )?;
                    if p(g.more)? {
                        t += 3;
                    }
                    t
                }
                GroupKind::Dyad => {
                    let n =
                        u8::from(p(g.pair_x)?) + u8::from(p(g.pair_y)?) + 3 * u8::from(p(g.more)?);
                    match n {
                        0 => b'1',
                        1 => b'2',
                        2 => b'4',
                        3 => b'3',
                        4 => b'5',
                        _ => {
                            return Err(CoreError::InvalidStructure(
                                "three prismatic pairs in a dyad",
                            ));
                        }
                    }
                }
            };
            self.groups[index].input_type = t;
        }
        Ok(())
    }
}
