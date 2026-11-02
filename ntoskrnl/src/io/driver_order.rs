//! Deterministic Windows-style driver load ordering.
//! The scheduler is allocation-free: the caller supplies the candidate service
//! set, registry-derived group/tag order, output indices, and scratch state.
#![allow(dead_code)]

#[derive(Clone,Copy,Debug)]
pub struct GroupTagOrder<'a>{pub group:&'a[u16],pub tags:&'a[u32]}

#[derive(Clone,Copy,Debug)]
pub struct ServiceNode<'a>{
    pub name:&'a[u16],
    pub group:&'a[u16],
    pub tag:Option<u32>,
    pub start_type:u32,
    pub service_dependencies:&'a[&'a[u16]],
    pub group_dependencies:&'a[&'a[u16]],
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ScheduleError{
    BufferTooSmall,InvalidStartType,EmptyName,DuplicateService,
    MissingServiceDependency,MissingGroupDependency,Cycle,
}

fn fold(c:u16)->u16{if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c}}
fn eq(a:&[u16],b:&[u16])->bool{a.len()==b.len()&&a.iter().zip(b).all(|(x,y)|fold(*x)==fold(*y))}
fn group_rank(group:&[u16],order:&[&[u16]])->usize{
    order.iter().position(|candidate|eq(group,candidate)).unwrap_or(usize::MAX)
}
fn tag_rank(group:&[u16],tag:Option<u32>,orders:&[GroupTagOrder<'_>])->(usize,u32){
    let Some(tag)=tag else{return (usize::MAX,u32::MAX)};
    if let Some(order)=orders.iter().find(|entry|eq(group,entry.group)){
        if let Some(index)=order.tags.iter().position(|candidate|*candidate==tag){return(index,tag)}
        return(usize::MAX,0)
    }
    (usize::MAX,0)
}
fn find_service(nodes:&[ServiceNode<'_>],name:&[u16])->Option<usize>{
    nodes.iter().position(|node|eq(node.name,name))
}
fn group_present(nodes:&[ServiceNode<'_>],group:&[u16])->bool{
    nodes.iter().any(|node|eq(node.group,group))
}
fn group_started(nodes:&[ServiceNode<'_>],state:&[u8],group:&[u16])->bool{
    nodes.iter().enumerate().any(|(i,node)|state[i]==1&&eq(node.group,group))
}
fn dependencies_ready(
    index:usize,nodes:&[ServiceNode<'_>],state:&[u8],initialized:&[&[u16]]
)->Result<bool,ScheduleError>{
    let node=&nodes[index];
    for dependency in node.service_dependencies{
        if let Some(dep)=find_service(nodes,dependency) {
            if dep==index{return Err(ScheduleError::Cycle)}
            if state[dep]!=1{return Ok(false)}
        } else if !initialized.iter().any(|name|eq(name,dependency)) {
            return Err(ScheduleError::MissingServiceDependency)
        }
    }
    for dependency in node.group_dependencies{
        let candidate_group=group_present(nodes,dependency);
        let initialized_group=nodes.iter().any(|node|initialized.iter().any(|name|eq(node.name,name))&&eq(node.group,dependency));
        if !candidate_group&&!initialized_group{return Err(ScheduleError::MissingGroupDependency)}
        if candidate_group&&!group_started(nodes,state,dependency)&&!initialized_group{return Ok(false)}
    }
    Ok(true)
}
fn precedes(a:usize,b:usize,nodes:&[ServiceNode<'_>],groups:&[&[u16]],tags:&[GroupTagOrder<'_>])->bool{
    let x=&nodes[a];let y=&nodes[b];
    let gx=group_rank(x.group,groups);let gy=group_rank(y.group,groups);
    if gx!=gy{return gx<gy}
    let tx=tag_rank(x.group,x.tag,tags);let ty=tag_rank(y.group,y.tag,tags);
    if tx!=ty{return tx<ty}
    a<b
}

/// Topologically order Boot/System-start drivers. Service dependencies must
/// name another candidate; group dependencies are satisfied when at least one
/// candidate in that group has initialized. Group order, then GroupOrderList
/// tag rank, then original enumeration order break ties deterministically.
pub fn schedule_boot_drivers(
    nodes:&[ServiceNode<'_>],
    group_order:&[&[u16]],
    tag_orders:&[GroupTagOrder<'_>],
    output:&mut[usize],
    state:&mut[u8],
)->Result<usize,ScheduleError>{
    schedule_boot_drivers_with_initialized(nodes,group_order,tag_orders,&[],output,state)
}

pub fn schedule_boot_drivers_with_initialized(
    nodes:&[ServiceNode<'_>],
    group_order:&[&[u16]],
    tag_orders:&[GroupTagOrder<'_>],
    initialized:&[&[u16]],
    output:&mut[usize],
    state:&mut[u8],
)->Result<usize,ScheduleError>{
    let count=nodes.len();
    if output.len()<count||state.len()<count{return Err(ScheduleError::BufferTooSmall)}
    state[..count].fill(0);
    for (index,node) in nodes.iter().enumerate() {
        if initialized.iter().any(|name|eq(node.name,name)) { state[index]=1; }
    }
    for (i,node) in nodes.iter().enumerate(){
        if node.name.is_empty(){return Err(ScheduleError::EmptyName)}
        if node.start_type>1{return Err(ScheduleError::InvalidStartType)}
        if nodes[..i].iter().any(|previous|eq(previous.name,node.name)){return Err(ScheduleError::DuplicateService)}
    }
    let target=count-state[..count].iter().filter(|value|**value==1).count();
    if output.len()<target{return Err(ScheduleError::BufferTooSmall)}
    let mut emitted=0usize;

    // Windows completes the entire SERVICE_BOOT_START pass before beginning
    // SERVICE_SYSTEM_START. A system-start candidate must never be pulled
    // forward merely because a boot-start dependency is unresolved.
    for phase in [0u32,1u32] {
        let phase_target=nodes.iter().enumerate()
            .filter(|(index,node)|state[*index]==0 && node.start_type==phase)
            .count();
        let phase_end=emitted+phase_target;
        while emitted<phase_end {
            let mut best:Option<usize>=None;
            for i in 0..count{
                if state[i]!=0 || nodes[i].start_type!=phase{continue}
                if !dependencies_ready(i,nodes,&state[..count],initialized)?{continue}
                if best.map(|current|precedes(i,current,nodes,group_order,tag_orders)).unwrap_or(true){best=Some(i)}
            }
            let Some(index)=best else{return Err(ScheduleError::Cycle)};
            state[index]=1;
            output[emitted]=index;
            emitted+=1;
        }
    }
    Ok(emitted)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn service_dependency_overrides_group_order(){
        let a=[65u16];let b=[66u16];let empty:&[&[u16]]=&[];let dep_a:[&[u16];1]=[&a];
        let nodes=[ServiceNode{name:&a,group:&[],tag:None,start_type:0,service_dependencies:empty,group_dependencies:empty},
          ServiceNode{name:&b,group:&[],tag:None,start_type:1,service_dependencies:&dep_a,group_dependencies:empty}];
        let mut out=[0usize;2];let mut state=[0u8;2];
        assert_eq!(schedule_boot_drivers(&nodes,&[],&[],&mut out,&mut state),Ok(2));
        assert_eq!(out,[0,1]);
    }
    #[test]
    fn all_boot_start_drivers_precede_system_start_drivers() {
        let boot=[b'B' as u16]; let system=[b'S' as u16];
        let early_group=[b'A' as u16]; let late_group=[b'Z' as u16];
        let nodes=[
            ServiceNode{name:&system,group:&early_group,tag:None,start_type:1,
                service_dependencies:&[],group_dependencies:&[]},
            ServiceNode{name:&boot,group:&late_group,tag:None,start_type:0,
                service_dependencies:&[],group_dependencies:&[]},
        ];
        let groups:[&[u16];2]=[&early_group,&late_group];
        let mut output=[usize::MAX;2]; let mut state=[0u8;2];
        assert_eq!(schedule_boot_drivers(&nodes,&groups,&[],&mut output,&mut state),Ok(2));
        assert_eq!(output,[1,0]);
    }

    #[test]
    fn group_order_and_tag_order_break_ties(){
        let a=[65u16];let b=[66u16];let g1=[71u16];let g2=[72u16];
        let nodes=[ServiceNode{name:&a,group:&g2,tag:Some(2),start_type:0,service_dependencies:&[],group_dependencies:&[]},
          ServiceNode{name:&b,group:&g1,tag:Some(9),start_type:1,service_dependencies:&[],group_dependencies:&[]}];
        let groups:[&[u16];2]=[&g1,&g2];let tag_values=[9u32,2u32];
        let tag_orders=[GroupTagOrder{group:&g1,tags:&tag_values[..1]},GroupTagOrder{group:&g2,tags:&tag_values[1..]}];
        let mut out=[0usize;2];let mut state=[0u8;2];
        assert_eq!(schedule_boot_drivers(&nodes,&groups,&tag_orders,&mut out,&mut state),Ok(2));
        assert_eq!(out,[1,0]);
    }
    #[test]
    fn rejects_missing_dependencies_and_cycles(){
        let a=[65u16];let b=[66u16];let dep_b:[&[u16];1]=[&b];
        let nodes=[ServiceNode{name:&a,group:&[],tag:None,start_type:0,service_dependencies:&dep_b,group_dependencies:&[]}];
        let mut out=[0usize;1];let mut state=[0u8;1];
        assert_eq!(schedule_boot_drivers(&nodes,&[],&[],&mut out,&mut state),Err(ScheduleError::MissingServiceDependency));
        let deps_a:[&[u16];1]=[&a];
        let cycle=[ServiceNode{name:&a,group:&[],tag:None,start_type:0,service_dependencies:&dep_b,group_dependencies:&[]},
          ServiceNode{name:&b,group:&[],tag:None,start_type:0,service_dependencies:&deps_a,group_dependencies:&[]}];
        assert_eq!(schedule_boot_drivers(&cycle,&[],&[],&mut [0usize;2],&mut [0u8;2]),Err(ScheduleError::Cycle));
    }
    #[test]
    fn preinitialized_platform_service_satisfies_dependency_without_scheduling_it(){
        let acpi=[65u16,67,80,73];
        let child=[67u16,72,73,76,68];
        let dependency:[&[u16];1]=[&acpi];
        let nodes=[ServiceNode{name:&acpi,group:&[],tag:None,start_type:0,
                service_dependencies:&[],group_dependencies:&[]},
            ServiceNode{name:&child,group:&[],tag:None,start_type:1,
                service_dependencies:&dependency,group_dependencies:&[]}];
        let initialized:[&[u16];1]=[&acpi];
        let mut output=[usize::MAX;2];let mut state=[0u8;2];
        assert_eq!(schedule_boot_drivers_with_initialized(
            &nodes,&[],&[],&initialized,&mut output,&mut state),Ok(1));
        assert_eq!(output[0],1);
        assert_eq!(state,[1,1]);
    }

    #[test]
    fn rejects_non_boot_system_start_candidates(){
        let a=[65u16];let nodes=[ServiceNode{name:&a,group:&[],tag:None,start_type:2,service_dependencies:&[],group_dependencies:&[]}];
        assert_eq!(schedule_boot_drivers(&nodes,&[],&[],&mut [0usize;1],&mut [0u8;1]),Err(ScheduleError::InvalidStartType));
    }
}
