use extendr_api::prelude::*;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::fs::{self, File};
use std::io::LineWriter;
use std::io::prelude::*;
use std::io::{self, BufRead};
use std::process::Command;
use rayon::prelude::*;

static ROOT_ID:&str = "138875005";
static N_PATIENTS:usize = 4652100;
/// Return string `"Hello world!"` to R.
/// @export
#[extendr]
fn hello_world() -> &'static str {
    "Hello world!"
}


fn parse_snomed_ff(filename:&str) -> HashMap<String, Vec<String>>{
    let mut relationship_map:HashMap<String, Vec<String>> = HashMap::new();
    let handle = File::open(filename).expect("could not open file");
    let stream = io::BufReader::new(handle).lines();

    for line in stream{
        let content = line.unwrap();
        let content = content.split("\t").collect::<Vec<&str>>();
        let src = String::from(content[4]);
        let dest = String::from(content[5]);
        // the correct way to check if key is in dict to mutate/add
        match relationship_map.entry(src){
            Entry::Vacant(children) => {children.insert(vec![dest]);},
            Entry::Occupied(mut children) =>{children.get_mut().push(dest);}
        }


    }
    return relationship_map
}


fn get_term_depth(mut query:String, relmap:&HashMap<String, Vec<String>>, limit:i64)-> i64{
    let mut count:i64 = 0;
    
    if &query == "NULL"{
        return -3
    }
    
    let mut parent = query.clone();

    if !relmap.contains_key(&query){
        return -1
    }
    let root_id = String::from(ROOT_ID);
    while parent != root_id{
        parent = relmap.get(&query).unwrap()[0].clone();
        count+=1;
        query = parent.clone();
        if count == limit{
            return -2
        }
    }

    return count
}

/// determine the depth(distance to root) of a term in the snomed hierarchy 
/// @export
#[extendr]
fn get_depth(relationship_file:String, terms:Vec<String>)->Vec<i64>{
    let relationship_map = parse_snomed_ff(&relationship_file);
    let depths = terms.into_iter()
                      .map(|x| get_term_depth(x, &relationship_map, 20))
                      .collect::<Vec<i64>>();
    return depths 
}


fn check_for_header(text:&str)->bool{
    let count = text.matches(",").count();
    return count == 10
}


fn _parse_mimic_noteevents(file:&str, tags:Vec<String>, keep_rawtext:bool, add_tags_to_bd:bool)-> Vec<HashMap<String, String>>{
    // parse freetext notes from mimic::NOTEEVENTS FILE
    // parser will look for lines starts specified in `tags`, ie "Final Diagnoses"
    // it then will store all text associated with that subnote in a single string
    // all other text belonging to subnotes not specified in `tag` are stored in "RAW_TEXT"
    // freetext has almost no formatting, so this parse makes the assumption that 
    // the end of a subnote is specified by a blank line. This is only true for some subnotes,
    // but not all. Always skim through a few examples of a subnote before including in `tags`
    let mut tags_distinct = tags.clone().into_iter().map(|x|x.to_lowercase()).collect::<Vec<String>>();
    tags_distinct.sort();
    tags_distinct.dedup();
    let base_headers = ["ROW_ID","SUBJECT_ID","HADM_ID","CHARTDATE","CHARTTIME","STORETIME","CATEGORY","DESCRIPTION","CGID","ISERROR","TEXT"];
    let mut base_keys:Vec<String> =  base_headers.iter().map(|x| x.to_string()).collect();
    base_keys.extend(tags_distinct.clone());

    let mut base_dict:HashMap<String, String> = HashMap::new();
    for key in base_keys{
        base_dict.insert(key.clone(), String::new());
    }
    // if add_tags_to_bd{
    //     for tag in tags.clone(){
    //         base_dict.insert(tag, String::new());
    //     }
    // }
    let patterns = tags.clone().into_iter().map(|x| format!("^{}:", x)).collect::<Vec<String>>(); // patterns to match
    let tag_patterns_re = regex::RegexSet::new(patterns).unwrap();
    let handle = File::open(file).expect("could not open file");
    let mut stream = io::BufReader::new(handle).lines();
    stream.next();// skip first line becasue it has a header we dont neeed
    let mut parsed_data:Vec<HashMap<String, String>> = vec![base_dict.clone();N_PATIENTS]; // initialize empty vector of results 
    let mut i:i32=-1;// this is to functionally allow us pre-initialize 
    let mut read_record_lines:bool= false;
    let mut is_target_category:bool = false;
    let mut c_record_buffer:Vec<String> = Vec::new();
    let mut raw_text_buffer:Vec<String> =Vec::new();
    let mut c_record_name = String::from("TEMP");
    for line in stream{
        let content = line.unwrap();
        if check_for_header(&content){ // we can identify headers by counting the number of commas ( each should have 10)
            if i>=0{
                // once within the loop, at this poitn we have reached a new row, so push raw text from previous row before incrementing 
                if keep_rawtext {
                    parsed_data[i as usize].insert(String::from("RAW_TEXT"), raw_text_buffer.join(" "));
                }
                // this is a hotfix, needs to be changed 
                if !(parsed_data[i as usize].contains_key(&tags_distinct[0]) || parsed_data[i as usize].contains_key(&tags_distinct[1])){
                    parsed_data[i as usize] = base_dict.clone();
                }
                c_record_name = String::from("TEMP");
            }
            // this is the start fo a new row; 
            // push header info to dict, then reset text buffers 
            i+=1;
            let mut base_header_values = content.split(",").collect::<Vec<&str>>();
            if base_header_values.len() > 11{
                //println!("header is longer than 11, keeping first 11 only ");
                base_header_values = base_header_values[0..11].to_vec();
            }
            for (j, value) in base_header_values.into_iter().enumerate(){
                parsed_data[i as usize].insert(base_headers[j].to_string(), value.to_string().replace("\"", ""));
            }
            if parsed_data[i as usize].get("CATEGORY").unwrap() != "Discharge summary"{
                is_target_category = false;
                parsed_data[i as usize] = base_dict.clone() ;
                continue
            } else{
                is_target_category = true;
            }
            c_record_buffer = Vec::new();
            raw_text_buffer = Vec::new();
            
        } else { 
            //check if line starts with the record start regex 
            let matches:Vec<_> = tag_patterns_re.matches(&content).into_iter().collect();
            if matches.len() >0 {
                // Begin recording a new line 
                c_record_name = tags[matches[0]].clone();

                read_record_lines = true && is_target_category; // 
                c_record_buffer = Vec::new();
            } else if read_record_lines {
                if &content == ""{ // we've reached the end of the subnote; push data to dict, and stop recording lines 
                    //
                    parsed_data[i as usize].insert(c_record_name.clone().to_lowercase(), c_record_buffer.join("|"));
                    if keep_rawtext{
                        parsed_data[i as usize].insert(String::from("RAW_TEXT"), raw_text_buffer.join(" "));
                    }
                    //println!("{}", c_record_buffer.join(" "));
                    read_record_lines =false && is_target_category;
                }else{
                    c_record_buffer.push(content);
                }
            } else{
                raw_text_buffer.push(content)
            }
        }
    }

    return parsed_data.into_iter().filter(|x| x != &base_dict ).collect::<Vec<HashMap<String, String>>>()
}

/// Return string `"Hello world!"` to R.
/// @export
#[extendr]
fn parse_mimic_noteevents(file:&str, tags:Vec<String>, keep_rawtext:bool, outfile:&str, add_tags_to_bd:bool) ->usize{

    let mut tags_distinct = tags.clone().into_iter().map(|x|x.to_lowercase()).collect::<Vec<String>>();
    tags_distinct.sort();
    tags_distinct.dedup();
    let base_headers = ["ROW_ID","SUBJECT_ID","HADM_ID","CHARTDATE","CHARTTIME","STORETIME","CATEGORY","DESCRIPTION","CGID","ISERROR","TEXT"];
    let mut base_keys:Vec<String> =  base_headers.iter().map(|x| x.to_string()).collect();
    base_keys.extend(tags_distinct.clone());
    let parsed_vhm = _parse_mimic_noteevents(file, tags, false, add_tags_to_bd);
    let parsed_vhm_fmtd = parsed_vhm.into_iter().map(|mut x| {
        base_keys.clone().iter().map(|j| x.remove(j).unwrap()).collect::<Vec<String>>().join(",")
    }).collect::<Vec<String>>();
  let outstring = parsed_vhm_fmtd.join("\n");
  fs::write(outfile, outstring);
  return 0
}

///  Parse note-events files, extract discharge diagnosis free text, extract UMLS CUIs with metamap, map cuis to SNOMED
/// @export
#[extendr]
fn get_freetext_diagnosis_as_snomed_ids(noteevents_file:&str, 
                                            record_tags:Vec<String>,
                                            umlsCUI_snomedID_mapfile: &str, 
                                            keep_types_list: Vec<String>,
                                            add_tags_to_bd:bool
                                            ) -> Vec<String>{
    let pool = rayon::ThreadPoolBuilder::new().num_threads(6).build().unwrap();
    let keep_types = keep_types_list.into_iter().collect::<HashSet<String>>();
    let mut tags_distinct = record_tags.clone().into_iter().map(|x|x.to_lowercase()).collect::<Vec<String>>();
    tags_distinct.sort();
    tags_distinct.dedup();


    let parsed_noteevents = _parse_mimic_noteevents(noteevents_file, record_tags, false, add_tags_to_bd);
    let parsed_umls_snomed_map:HashMap<String, HashSet<String>> = parse_UMLS_SNOMED_mapping(umlsCUI_snomedID_mapfile);
    //println!("{} records to process", parsed_noteevents.len())
    
    let text_mapped:Vec<String> = pool.install(|| parsed_noteevents
                             .into_par_iter()
                             .enumerate()
                             .map(|(i, parsed_record)| {
                                if i % 1000 == 0{
                                    println!("{} records done", i);
                                } 
                                run_metamap(parsed_record, tags_distinct.clone(), &keep_types, &parsed_umls_snomed_map )})
                             .flatten()
                             .collect() 
                        );
    
    // let text_mapped = parsed_noteevents
    //                   .into_iter()
    //                   .map(|parsed_record| )
    //                   .flatten()
    //                   .collect::<Vec<String>>() ;   
    return text_mapped
}



fn parse_UMLS_SNOMED_mapping(umlsCUI_snomedID_mapfile:&str) -> HashMap<String, HashSet<String>>{
    let mut mapping: HashMap<String, HashSet<String>> = HashMap::new();
    let handle = File::open(umlsCUI_snomedID_mapfile).expect("could not open file");
    let stream = io::BufReader::new(handle).lines();
    for line in stream{
        let content = line.unwrap();
        let content = content.split("|").collect::<Vec<&str>>();
        let cui = String::from(content[0]);
        let sid = String::from(content[1]);
        match mapping.entry(cui){
            Entry::Vacant(value) => {let mut s: HashSet<String> = HashSet::new();
                                     s.insert(sid);
                                     value.insert( s );},
            Entry::Occupied(mut value) =>{value.get_mut().insert(sid);}
        }
    }
    
    return mapping
}

fn format_parsed_record_input(record: HashMap<String, String>,tag_keys:Vec<String> ) -> Vec<String>{
    let pid = record.get("SUBJECT_ID").unwrap().clone();
    let hadmid = record.get("HADM_ID").unwrap().clone();
    let uid = format!("{}-{}", pid, hadmid);
    let available_keys = tag_keys.into_iter().filter(|x| record.contains_key(x)).collect::<Vec<String>>();
    let mut target_key = String::new();
    if available_keys.len() >= 2{
        //println!("WARNING: More than one record tag available({}, {} ) for record {}. Defaulting to first available key", available_keys[0], available_keys[1], uid);
        target_key = available_keys[0].clone()
    } else if available_keys.len() == 1{
        target_key = available_keys[0].clone()
    } else{
        println!(" WARNING:No Available key for record {}", uid);
        return vec![format!("{}|.", uid)] 
    }
    let record_text = record.get(&target_key).unwrap();
    //let record_text_clone = record_text.clone().replace(",", "").replace("|", ",");
    let out_str = record_text.split("|").enumerate().map(|(i, line)| {
                                format!("{}-{}|{}", uid, i, line)
    } ).collect::<Vec<String>>();
    return out_str
}

///  Parse note-events files, extract discharge diagnosis free text, extract UMLS CUIs with metamap, map cuis to SNOMED
/// @export
#[extendr]
fn get_freetext_diagnosis_forbatchmm(noteevents_file:&str, 
                                            record_tags:Vec<String>,
                                            umlsCUI_snomedID_mapfile: &str,
                                            keep_types_list: Vec<String>, 
                                            outfile: String,
                                            add_tags_to_bd:bool
                                            ) -> usize{
    
    let keep_types = keep_types_list.into_iter().collect::<HashSet<String>>();
    let mut tags_distinct = record_tags.clone().into_iter().map(|x|x.to_lowercase()).collect::<Vec<String>>();
    tags_distinct.sort();
    tags_distinct.dedup();                                              
    let parsed_noteevents = _parse_mimic_noteevents(noteevents_file, record_tags, false, false);

    //println!("{}",parsed_noteevents_filtered.len());
    let parsed_umls_snomed_map:HashMap<String, HashSet<String>> = parse_UMLS_SNOMED_mapping(umlsCUI_snomedID_mapfile);
    //println!("{} records to process", parsed_noteevents.len())
    
    //let bar = ProgressBar::new(parsed_noteevents_filtered.len() as u64);
    let file = File::create(outfile).expect("couldnt create outfile ");
    let mut outwriter = LineWriter::new(file);

    println!("premap {}", parsed_noteevents.len());
    let text_mapped = parsed_noteevents
                      .into_iter()
                      .enumerate()
                      .map(|(i, parsed_record)|{ //bar.inc(1);
                        let res = format_parsed_record_input(parsed_record, tags_distinct.clone());
                        let outstr = res.join("\n");
                        let outstr = format!("{}\n", outstr);
                        outwriter.write_all(outstr.as_bytes()).expect("ould not write ");
                        if i % 1000  == 0{
                            println!("{} records done", i);
                        }
                        return 0
                    }).collect::<Vec<usize>>();
                      ; 
    //println!("postmap");
    outwriter.flush().expect("flush?");
    return 0
}

fn run_metamap<'a>(record: HashMap<String, String>, tag_keys:Vec<String>, keep_types :&HashSet<String>, cui2snomed:&HashMap<String, HashSet<String>>) -> Vec<String> {
    let pid = record.get("SUBJECT_ID").unwrap().clone();
    let hadmid = record.get("HADM_ID").unwrap().clone();
    let uid = format!("{}|{}", pid, hadmid);
    let available_keys = tag_keys.into_iter().filter(|x| record.contains_key(x)).collect::<Vec<String>>();
    let mut target_key = String::new();
    if available_keys.len() >= 2{
        //println!("WARNING: More than one record tag available({}, {} ) for record {}. Defaulting to first available key", available_keys[0], available_keys[1], uid);
        target_key = available_keys[0].clone()
    } else if available_keys.len() == 1{
        target_key = available_keys[0].clone()
    } else{
        println!(" WARNING:No Available key for record {}", uid);
        return vec![format!("{}|.", uid)] 
    }
    let record_text = record.get(&target_key).unwrap();
    //let record_text_clone = record_text.clone().replace(",", "").replace("|", ",");
    let out_str = record_text.split("|").map(|line| {
                                let mapping_string =  call_metamap(line,uid.clone(), keep_types, cui2snomed);
                                format!("{}|{}", uid, mapping_string)
    } ).collect::<Vec<String>>();

    return out_str
}

fn call_metamap(text:&str, uid:String,  keep_types:&HashSet<String>, cui2snomed:&HashMap<String, HashSet<String>>) -> String{
    let raw_line = String::from(text);
    let text = format!("{}", text);
    fs::write(".mm.txt", text).expect("Failed to write ");
    let output = Command::new("bash")
                     .arg("/Users/vinayswamy/run_metamap.sh")
                     .arg(".mm.txt")
                     .output()
                     .expect("Failed to execute command");
    
    /*
    For reference the script run_metamap.sh looks like this 
    #!/bin/bash
    set -e 
    infile=$1
    cdir=$PWD
    metamapexec_dir="/Users/vinayswamy/columbia/coursework/BINFG4003-symbolic-methods/project/MetaMap_exec/"
    metamapindex_dir="/Users/vinayswamy/columbia/coursework/BINFG4003-symbolic-methods/project/MetaMap_data/data/ivf/2020AA/USAbase/"
    cp $infile $metamapexec_dir
    cd $metamapexec_dir
    bash metamaplite.sh --overwrite --indexdir=$metamapindex_dir $infile
    cp ${infile/txt/mmi} $cdir
    
    */
    let res_str = fs::read_to_string(".mm.mmi").expect("failed to read") ;
    
    let op = Command::new("rm")
                      .arg(".mm.mmi")
                      .output()
                      .expect("failed to remove mmi");
    let mut res : Vec<String> = res_str.split("\n").map(|x| String::from(x)).collect();
    res.pop();
    let CUIS : Vec<String> = res.into_iter().map(|x| parse_mm_output_string(x, keep_types) ).filter(|x| x != &String::new()).collect();
    let CUI_str = CUIS.clone().join(",");
    // let SIDS = CUIS.iter().map(|x| { let sid = cui2snomed.get(x);
    //     match sid {
    //         Some(x) =>  x.clone().into_iter().collect::<Vec<String>>().join(","),
    //         None => String::new()
    //     }}).collect::<Vec<String>>().join(",");
    let out_str = format!("{}|{}|{}|", uid, raw_line,CUI_str);
    return out_str
}

fn parse_mm_output_string(line:String, keep_types:&HashSet<String>) -> String{
    let content : Vec<String> = line.split("|").map(|x| String::from(x)).collect();
    let cui = &content[4];
    let x = String::from(&content[5]) ;
    let semantic_type = String::from(&x[1..5]);
    if keep_types.contains(&semantic_type){
        return cui.clone() ;
    } else{
        return String::new();
    }
}







///  Parse note-events files, extract discharge diagnosis free text, extract UMLS CUIs with metamap, map cuis to SNOMED
/// @export
#[extendr]
fn get_freetext_diagnosis_as_snomed_ids_filter_and_write(noteevents_file:&str, 
                                            record_tags:Vec<String>,
                                            umlsCUI_snomedID_mapfile: &str,
                                            keep_types_list: Vec<String>, 
                                            outfile: String,
                                            add_tags_to_bd:bool
                                            ) -> usize{
    
    let keep_types = keep_types_list.into_iter().collect::<HashSet<String>>();
    let mut tags_distinct = record_tags.clone().into_iter().map(|x|x.to_lowercase()).collect::<Vec<String>>();
    tags_distinct.sort();
    tags_distinct.dedup();                                              
    let parsed_noteevents = _parse_mimic_noteevents(noteevents_file, record_tags, false, false);

    //println!("{}",parsed_noteevents_filtered.len());
    let parsed_umls_snomed_map:HashMap<String, HashSet<String>> = parse_UMLS_SNOMED_mapping(umlsCUI_snomedID_mapfile);
    //println!("{} records to process", parsed_noteevents.len())
    
    //let bar = ProgressBar::new(parsed_noteevents_filtered.len() as u64);
    let file = File::create(outfile).expect("couldnt create outfile ");
    let mut outwriter = LineWriter::new(file);

    println!("premap {}", parsed_noteevents.len());
    let text_mapped = parsed_noteevents
                      .into_iter()
                      .enumerate()
                      .map(|(i, parsed_record)|{ //bar.inc(1);
                        let res = run_metamap(parsed_record, tags_distinct.clone(), &keep_types, &parsed_umls_snomed_map );
                        let outstr = res.join("\n");
                        let outstr = format!("{}\n", outstr);
                        outwriter.write_all(outstr.as_bytes()).expect("ould not write ");
                        if i % 1000  == 0{
                            println!("{} records done", i);
                        }
                        return 0
                    }).collect::<Vec<usize>>();
                      ; 
    //println!("postmap");
    outwriter.flush().expect("flush?");
    return 0
}
// Macro to generate exports.
// This ensures exported functions are registered with R.
// See corresponding C code in `entrypoint.c`.
extendr_module! {
    mod mimicR;
    fn hello_world;
    fn get_depth;
    fn parse_mimic_noteevents;
    fn get_freetext_diagnosis_as_snomed_ids;
    fn get_freetext_diagnosis_as_snomed_ids_filter_and_write ;
    fn get_freetext_diagnosis_forbatchmm;
}


/*
Goal Extract diagnosis Note events from free text, NLP with metamap, convert metamap CUI to SNOMED SIDS return vector of SIDS


End return type from rust: Vec<str>(compatible with extendr) 
where each entry is formatted as <PatientID>|<HADMID>|<SID1,SID2...>


tasks:

Get_SNOMED_from_text(noteevents_file, cui2_snomed_file ) -> Vec<&str>
    res
    cui2_snomed_file<HashMap<String,HashSet<String>>> = parse_snomed2cuimap
    for distinct_record in notevents file
        res = text2snomed(distinct_record, cui2snomed)
    res+= PID+HADMID+res

text2snomed(distinct_record, cui2snomed) ->&str
    res = ""
    for line in distinct record:
        res + =run_metamap(line, cui2snomed)
    res.join(,)
    return res 


run_metamap(line, cui2snomed) ->
         CUIS = process(metamap)
         SIDS  = cui2snomed.get queryie( CUIS)
        r
parse_snomed2cuimap(cui2snomed_file) > HashMap
        

        */


