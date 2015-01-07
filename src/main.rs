#![feature(slicing_syntax)]
extern crate git2;

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::args;
use git2::{Branch, BranchType, Commit, ResetType, ObjectType, Oid, Repository, Signature, Tree};

fn main() {
    for path_str in args()[1..].iter() {
        let mut store: HashMap<Oid, Data> = HashMap::new();
        let repo = get_repository(path_str[]);
        let mut roots = Vec::new();
        for branch in get_branches(&repo).into_iter() {
            populate_from_branch(branch, &repo, &mut store, &mut roots)
        }
        for data in store.values() {
            for parent in data.commit.parents() {
                store[parent.id()].children.borrow_mut().push(data.commit.id())
            }
        }
        for oid in roots.iter() {
            let tree = repo.find_tree(repo.find_commit(*oid).unwrap().tree_id()).unwrap();
            rebuild(*oid, &repo, &tree, &store);
        }
        for branch in get_branches(&repo).into_iter() {
            let new_oid = store[branch.get().target().unwrap()].new_commit.borrow().as_ref().unwrap().id();
            if !branch.is_head() {
                let new_commit = repo.find_commit(new_oid).unwrap();
                repo.branch(branch.name().unwrap().unwrap(), &new_commit, true, None, None).unwrap();
            } else {
                let new_commit = repo.find_object(new_oid, Some(ObjectType::Commit)).unwrap();
                repo.reset(&new_commit, ResetType::Hard, None, None).unwrap();
            }
        }
    }
}

fn get_repository(path_str: &str) -> Repository {
    match Repository::init(&Path::new(path_str)) {
        Ok(repo) => repo,
        Err(e) => panic!("failed to init `{}`: {}", path_str, e),
    }
}

fn get_branches<'a>(repo: &'a Repository) -> Vec<Branch<'a>> {
    repo.branches(Some(BranchType::Local)).unwrap().map(|tup| tup.0).collect()
}

fn populate_from_branch<'a>(branch: Branch<'a>, repo: &'a Repository, store: &mut HashMap<Oid, Data<'a>>, roots: &mut Vec<Oid>) {
    populate(repo.find_commit(branch.into_reference().target().unwrap()).unwrap(), store, roots);
}

fn populate<'a>(commit: Commit<'a>, store: &mut HashMap<Oid, Data<'a>>, roots: &mut Vec<Oid>) {
    let oid = commit.id();
    if !store.contains_key(&oid) {
        store.insert(commit.id(), Data::new(commit));
    }
    if (&mut store[oid]).commit.parents().next().is_none() {
        roots.push(oid);
    } else {
        let parents: Vec<Commit> = (&mut store[oid]).commit.parents().collect();
        for parent in parents.into_iter() {
            populate(parent, store, roots);
        }
    }
}

fn rebuild<'a>(oid: Oid, repo: &'a Repository, tree: &Tree<'a>, store: &'a HashMap<Oid, Data<'a>>) {
    let ref data = store[oid];
    let is_rebuilt = |&: c: Commit, store: &HashMap<Oid, Data>| {
        store[c.id()].new_commit.borrow().is_some()
    };
    if data.new_commit.borrow().is_none() && data.commit.parents().all(|c| is_rebuilt(c, store)) {
        if data.commit.author().name().unwrap() != "Anonymous" &&
           data.commit.author().email().unwrap() != "anon@ymo.us" &&
           data.commit.committer().name().unwrap() != "Anonymous" &&
           data.commit.committer().email().unwrap() != "anon@ymo.us" {
               let author = Signature::new("Anonymous", "anon@ymo.us", 
                                           data.commit.author().when().seconds() as u64,
                                           data.commit.author().when().offset_minutes());
               let committer = Signature::new("Anonymous", "anon@ymo.us",
                                              data.commit.committer().when().seconds() as u64, 
                                              data.commit.committer().when().offset_minutes());
               let message = data.commit.message().unwrap();
               let parents_vals = get_parents(oid, repo, store);
               let parents: Vec<_> = parents_vals.iter().map(|commit| commit).collect();
               let new_oid = repo.commit(None, &author.unwrap(), &committer.unwrap(), message,
                            tree, parents[]).unwrap();
               {
                   let mut new_commit = data.new_commit.borrow_mut();
                   *new_commit = Some(repo.find_commit(new_oid).unwrap());
               }
               let new_tree = repo.find_tree(repo.find_commit(new_oid).unwrap().tree_id()).unwrap();
               for child_oid in data.children.borrow().iter() {
                   rebuild(*child_oid, repo, &new_tree, store);
               }
        } else {
            for child_oid in data.children.borrow().iter() {
                rebuild(*child_oid, repo, tree, store);
            }
        }
    }
}

fn get_parents<'a>(oid: Oid, repo: &'a Repository, store: &'a HashMap<Oid, Data<'a>>) -> Vec<Commit<'a>> {
    store[oid].commit.parents().map(|commit| {
        repo.find_commit(store[commit.id()].new_commit.borrow().as_ref().unwrap().id()).unwrap()
    }).collect()
}

struct Data<'a> {
    commit: Commit<'a>,
    children: RefCell<Vec<Oid>>,
    new_commit: RefCell<Option<Commit<'a>>>,
}

impl<'a> Data<'a> {
    pub fn new(commit: Commit<'a>) -> Data {
        Data {
            commit: commit,
            children: RefCell::new(Vec::new()),
            new_commit: RefCell::new(None),
        }
    }
}
