#%%
import pandas as pd
import networkx as nx
# %%
mapping = pd.read_csv("snomed_is-a_relationships.txt", header=None, sep = '\t',dtype=str )
# %%
edges = list(mapping.iloc[:,[5, 4]].to_records(index = False))
# %%
G = nx.DiGraph()
# %%
G.add_edges_from(edges)

# %%
nx.algorithms.dag.is_directed_acyclic_graph(G)
# %%
paths=nx.algorithms.shortest_paths.generic.shortest_path(G, "138875005")
# %%
path_lengths = pd.DataFrame([(p, len(paths[p]) ) for p in paths.keys()], columns = ['SID', 'depth'])
path_lengths.depth.describe()
path_lengths.to_csv('SNOMED_term_depth_from_networkx.csv', index = False)
# %%
