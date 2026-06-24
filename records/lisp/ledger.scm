(define-class (ledger)
  ;; Ledger class manages config, staged state, and signed/pinned chain history.
  (define-method (*init* self standard (config '()) tree-class chain-class document-class)
    ;; Initialize ledger with helper objects, inline config data, and fresh storage classes.
    ;;   Args:
    ;;     standard (standard object): standard helper instance.
    ;;     config (list): initial config expression.
    ;;     tree-class (list): tree class definition.
    ;;     chain-class (list): chain class definition.
    ;;     document-class (list): document class definition.
    ;;   Returns:
    ;;     boolean: #t after setting fields.
    ((self '~field!) 'standard standard)
    ((self '~field!) 'config (expression->byte-vector config))
    (if (null? ((self '~config-get) '(public bridge-policy)))
        ((self '~config-set!) '(public bridge-policy) '((publish push) (subscribe pull))))
    (let ((standard-obj (sync-eval standard #f)))
      ((self '~field!) 'document ((standard-obj 'make) document-class))
      ((self '~field!) 'stage ((standard-obj 'init) tree-class))
      ((self '~field!) 'temp ((standard-obj 'init) chain-class))
      ((self '~field!) 'perm ((standard-obj 'init) chain-class))))

  (define-method (config self (path '()))
    ;; Return inline ledger config data.
    ;;   Args:
    ;;     path (list): optional nested config path.
    ;;   Returns:
    ;;     list: config expression or subexpression.
    ((self '~config-get) path))

  (define-method (info self (subscriber #f))
    ;; Return public config info only, including bridge policy.
    ;;   Args:
    ;;     subscriber (symbol): optional subscriber name for future per-peer effective policy.
    ;;   Returns:
    ;;     list: public config expression.
    ((self '~config-get) '(public)))

  (define-method (size self)
    ;; Size of the permanent chain.
    ;;   Returns:
    ;;     integer: chain size.
    (let ((perm (sync-eval ((self '~field!) 'perm) #f)))
      ((perm 'size))))

  (define-method (bridge! self name info-local info-remote)
    ;; Apply prepared bridge/publication data and cache public key/policy.
    ;;   Args:
    ;;     name (symbol): local bridge/publication target name.
    ;;     info-local (alist): locally chosen bridge data such as interface, policy, role, and remote-name.
    ;;     info-remote (list): remote public info payload.
    ;;   Returns:
    ;;     boolean: #t after updating config, #f when policy disables the bridge/publication.
    (let* ((public-key (cadr (assoc 'public-key info-remote)))
           (remote-policy (cadr (assoc 'bridge-policy info-remote)))
           (local-policy (cadr (assoc 'policy info-local))))
      (if (eq? (cadr (assoc 'role info-local)) 'publisher)
          (let ((mode ((self '~bridge-mode) local-policy remote-policy)))
            (if (eq? mode 'none)
                (begin ((self '~config-set!) `(private subscriber ,name) '()) #f)
                (begin
                  ((self '~config-set!) `(private subscriber ,name) info-remote)
                  ((self '~config-set!) `(private subscriber ,name local) info-local)
                  ((self '~config-set!) `(private subscriber ,name interface) (cadr (assoc 'interface info-local)))
                  ((self '~config-set!) `(private subscriber ,name public-key) public-key)
                  ((self '~config-set!) `(private subscriber ,name remote-name) (cadr (assoc 'remote-name info-local)))
                  ((self '~config-set!) `(private subscriber ,name policy local) local-policy)
                  ((self '~config-set!) `(private subscriber ,name policy remote) remote-policy)
                  ((self '~config-set!) `(private subscriber ,name policy mode) mode)
                  #t)))
          (let ((mode ((self '~bridge-mode) remote-policy local-policy)))
            (if (eq? mode 'none)
                (begin ((self '~delete-bridge!) name) #f)
                (begin
                  ((self '~config-set!) `(private bridge ,name) info-remote)
                  ((self '~config-set!) `(private bridge ,name local) info-local)
                  ((self '~config-set!) `(private bridge ,name interface) (cadr (assoc 'interface info-local)))
                  ((self '~config-set!) `(private bridge ,name public-key) public-key)
                  ((self '~config-set!) `(private bridge ,name policy local) local-policy)
                  ((self '~config-set!) `(private bridge ,name policy remote) remote-policy)
                  ((self '~config-set!) `(private bridge ,name policy mode) mode)
                  #t))))))

  (define-method (get self path meta? expression?)
    ;; Get value at a stage path.
    ;;   Args:
    ;;     path (list): path segments.
    ;;     meta? (boolean): if #t, include document metadata envelope.
    ;;     expression? (boolean): if #t, decode document payload bytes as an expression.
    ;;   Returns:
    ;;     any: document byte-vector/expression value, metadata envelope, sentinel, or directory listing.
    (set! path ((self '~path-normalize) path #t #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (stage ((self '~field!) 'stage))
           (node ((standard 'deep-get) stage path)))
      (if ((self '~bridge-chain-value-path?) path) node
          ((self '~document-read) node meta? expression?))))

  (define-method (set! self path value (meta '()) expression?)
    ;; Stage a document state change at path (not yet committed).
    ;;   Args:
    ;;     path (list): path segments.
    ;;     value: byte-vector payload, expression payload when expression? is #t, or `(nothing)` to delete.
    ;;     meta (alist): metadata patch, `()`, or `(nothing)`.
    ;;     expression? (boolean): if #t, encode value as an expression before storing bytes.
    ;;   Returns:
    ;;     boolean: #t after staging.
    (let ((public-path path))
      (set! path ((self '~path-normalize) path #t #t))
      (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
             (stage-obj (sync-eval ((self '~field!) 'stage) #f))
             (existing ((standard 'deep-get) (stage-obj) path))
             (document ((self '~document-write) existing value meta expression?)))
        ((stage-obj 'copy!) '(*transition*) '(*transition* previous))
        ((stage-obj 'set!) '(*transition* operation)
         `((path ,public-path) (value ,value) ,@(if expression? `((expression? #t)) '()) ,@(if (null? meta) '() `((meta ,meta)))))
        (if (> (length path) 2)
            ((self '~field!) 'stage ((standard 'deep-set!) (stage-obj) path document))
            (begin ((stage-obj 'set!) (car path) document)
                   ((self '~field!) 'stage (stage-obj)))))))

  (define-method (set-batch! self paths values (metas '()) expression?)
    ;; Stage multiple document value changes with optional metadata patches.
    ;;   Args:
    ;;     paths (list): document paths.
    ;;     values (list): document values.
    ;;     metas (list): optional metadata patches parallel to paths/values.
    ;;     expression? (boolean): if #t, encode each value as an expression before storing bytes.
    ;;   Returns:
    ;;     boolean: #t after staging.
    (let ((metas (if (null? metas) (make-list (length paths) '()) metas)))
      (let loop ((paths paths) (values values) (metas metas))
        (cond ((and (null? paths) (null? values) (null? metas)) #t)
              ((or (null? paths) (null? values) (null? metas))
               (error 'argument-error "Paths, values, and metadata lists must have equal length: ~S ~S ~S" paths values metas))
              (else ((self 'set!) (car paths) (car values) (car metas) expression?)
                    (loop (cdr paths) (cdr values) (cdr metas)))))))

  (define-method (resolve self path pinned? proof? head meta? expression?)
    ;; Get value at path, optionally with metadata and proof details.
    ;;   Args:
    ;;     path (list): path segments.
    ;;     pinned? (boolean): include pinned detail.
    ;;     proof? (boolean): include proof detail.
    ;;     head (sync node): optional prepared chain head.
    ;;     meta? (boolean): if #t, include document metadata envelope.
    ;;     expression? (boolean): if #t, decode document payload bytes as an expression.
    ;;   Returns:
    ;;     any: byte-vector/expression value, metadata envelope, or association list with content/pinned?/proof.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (head (if head head ((self '~head) path)))
           (node ((standard 'deep-get) head path))
           (document? (and (eq? ((self '~object-type) node) 'document)
                           (not ((self '~bridge-chain-value-path?) path))))
           (content (if ((self '~bridge-chain-value-path?) path) node ((self '~document-read) node meta? expression?)))
           (proof-path (if document? (append path (list ((self '~document-field) meta?))) path)))
      (if (not (or pinned? proof?)) content
          `((content ,content)
            ,@(if (not pinned?) '()
                  (let* ((perm-node ((standard 'deep-get) ((self '~field!) 'perm) path))
                         (perm-content (if ((self '~bridge-chain-value-path?) path)
                                           perm-node
                                           ((self '~document-read) perm-node meta? expression?))))
                    `((pinned? ,(not (equal? perm-content '(unknown)))))))
            ,@(if (not proof?) '()
                  (let ((proof ((standard 'deep-slice!) head proof-path)))
                    `((proof ,((standard 'serialize) proof)))))))))

  (define-method (bridge-head self path (index -1))
    ;; Describe the remote bridge head needed to continue resolving a flat path.
    ;;   Args:
    ;;     path (list): flat bridge path being resolved.
    ;;     index (integer): local history index to inspect.
    ;;   Returns:
    ;;     alist: remote interface, trace index, and remaining flat path.
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (path~ ((self '~path-normalize) path #f #f))
           (segments (if (and (pair? path) (integer? (car path))) (cdr path) path))
           (name (cadr segments))
           (interface ((self '~config-get) `(private bridge ,name interface)))
           (local-index (car path~))
           (local-chain (((sync-eval ((self 'resolve) '()) #f) 'previous) index))
           (remote-chain ((standard 'deep-get) local-chain `(,local-index (*bridge* ,name chain))))
           (remote-index (- (((sync-eval remote-chain #f) 'size)) 1))
           (remote-path (cddr segments)))
      `((interface ,interface)
        (index ,remote-index)
        (path ,remote-path))))

  (define-method (merge-head self path head (index -1))
    ;; Merge a fetched remote bridge head into the local chain head for a flat path.
    ;;   Args:
    ;;     path (list): flat bridge path being resolved.
    ;;     head (sync node): remote head fetched by the interface.
    ;;     index (integer): local history index to merge against.
    ;;   Returns:
    ;;     sync node: merged local chain head.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (chain ((self 'resolve) '()))
           (local-chain (((sync-eval chain #f) 'previous) index))
           (remote-chain ((standard 'deep-get) local-chain (list (car path) (cadr path))))
           (remote-path (list-tail path 2))
           (remote-bridge? (and (> (length remote-path) 1)
                                (pair? (cadr remote-path))
                                (eq? (caadr remote-path) '*bridge*)
                                (> (length (cadr remote-path)) 1)))
           (prefix (reverse (list-tail (reverse path) (- (length path) 2)))))
      (if (and (not remote-bridge?)
               (not (equal? (sync-digest remote-chain) (sync-digest head))))
          (error 'integrity-error "Remote chain does not match local bridge head for path: ~S" path)
          ((standard 'deep-merge!) head local-chain prefix))))

  (define-method (pin! self path response)
    ;; Merge a prepared pinned proof object into the permanent chain.
    ;;   Args:
    ;;     response (list): serialized object returned by `pin~`.
    ;;   Returns:
    ;;     boolean: #t after pinning.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (perm (sync-eval ((self '~field!) 'perm) #f))
           (window ((self '~config-get) '(public window)))
           (current (- ((perm 'size)) 1))
           (target ((perm 'index) (car path))))
      (if (and window (< target (- current window))) #f
          (let ((head-node (if response ((standard 'deserialize) response)
                               (let* ((head ((self '~head) path))
                                      (node ((standard 'deep-get) head path)))
                                 (if (or (equal? node '(nothing)) (equal? node '(unknown))) #f
                                     (let ((proof-path (if (and (eq? ((self '~object-type) node) 'document)
                                                                (not ((self '~bridge-chain-value-path?) path)))
                                                           (append path '(value)) path)))
                                       ((standard 'deep-slice!) head proof-path)))))))
            (if (not head-node) #f
                (let* ((head-obj (sync-eval head-node #f))
                       (head-index ((head-obj 'index) (car path))))
                  ((self '~field!) 'perm
                   ((standard 'deep-merge!) ((head-obj 'get) (car path)) ((self '~field!) 'perm) `(,head-index)))))))))

  (define-method (unpin! self path)
    ;; Remove a path from the permanent chain.
    ;;   Args:
    ;;     path (list): path segments.
    ;;   Returns:
    ;;     boolean: #t after unpinning.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (perm ((self '~field!) 'perm)))
      ((self '~field!) 'perm ((standard 'deep-prune!) perm path))))

  (define-method (synchronize self index)
    ;; Serialize bridge-visible chain digest at index for sync.
    ;;   Args:
    ;;     index (integer): index to access.
    ;;   Returns:
    ;;     list: serialization list.
    (let ((standard (sync-eval ((self '~field!) 'standard) #f))
          (perm (sync-eval ((self '~field!) 'perm) #f)))
      ((standard 'serialize) (perm)
       `(lambda (node)
          (let ((chain (sync-eval node #f)))
            (if (> ((chain 'size)) 0)
                (let* ((node ((chain 'get) -1))
                       (tree (sync-eval node #f)))
                  ((tree 'get) '(*crypto* public-key))
                  ((tree 'get) '(*crypto* signature))
                  ((chain 'digest) ,index))))))))

  (define-method (trace self index path head (meta? #f))
    ;; Trace a remote path against a serialized chain at index.
    ;;   Args:
    ;;     index (integer): index to access.
    ;;     path (list): path segments.
    ;;   Returns:
    ;;     list: serialization list of the traced object.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (head (if head head ((self '~head) path index)))
           (node ((standard 'deep-get) head path))
           (trace-path (if (and (eq? ((self '~object-type) node) 'document)
                                (not ((self '~bridge-chain-value-path?) path)))
                           (append path (list ((self '~document-field) meta?))) path)))
      ((standard 'serialize) head
       `(lambda (node)
          (letrec ((deep-get (lambda (node path)
                               (if (null? path) node
                                   (let ((child (((sync-eval node) 'get) (car path))))
                                     (if (not (sync-node? child)) child
                                         (deep-get child (cdr path))))))))
            (deep-get node ',trace-path))))))
  
  (define-method (synchronize! self name index response info interface (local-policy '()))
    ;; Receive a pushed bridge synchronization payload into staged bridge state.
    ;;   Args:
    ;;     name (symbol): configured or proposed bridge peer name.
    ;;     index (integer): subscriber's last synchronized index for this peer.
    ;;     response (list): pushed synchronization payload.
    ;;     info (list): optional publisher info for optimistic first-push bootstrap.
    ;;     interface (string): optional publisher interface for optimistic bootstrap.
    ;;     local-policy (alist): optional local policy override for optimistic bootstrap.
    ;;   Returns:
    ;;     list: acknowledgement metadata.
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (latest ((standard 'deserialize) response))
           (latest-obj (sync-eval latest #f))
           (pushed-index (if (> ((latest-obj 'size)) 0) (- ((latest-obj 'size)) 1) -1))
           (existing ((self '~config-get) `(private bridge ,name)))
           (public-key (if (null? existing) (and info (cadr (assoc 'public-key info)))
                           ((self '~config-get) `(private bridge ,name public-key)))))
      (if (not public-key)
          (error 'bridge-name-error "Unknown pushed bridge name and no publisher info supplied: ~S" name))
      ((self '~signature-verify) latest public-key)
      (if (null? existing)
          (begin
            (if (not ((self 'bridge!) name
                      `((interface ,(if interface interface ""))
                        (policy ,(if (null? local-policy) ((self '~config-get) '(public bridge-policy)) local-policy))
                        (role #f)
                        (remote-name ,name))
                      info))
                (error 'bridge-mode-error "Local policy does not allow pushed publication: ~S" name)))
          (if (and info (not (equal? public-key (cadr (assoc 'public-key info)))))
              (error 'bridge-name-error "Pushed bridge name is already bound to another public key: ~S" name)))
      (let ((mode ((self '~config-get) `(private bridge ,name policy mode)))
            (current-index ((self '~config-get) `(private bridge ,name last-index))))
        (if (not (eq? mode 'push))
            (error 'bridge-mode-error "Bridge is not negotiated for push: ~S mode=~S" name mode))
        (if (and (integer? current-index) (<= pushed-index current-index))
            (error 'bridge-sync-error "Pushed bridge payload is not newer for peer: ~S pushed=~S current=~S" name pushed-index current-index))
        (let ((applied ((self 'bridge-synchronize!) name index response)))
          (if (not applied)
              (error 'bridge-sync-error "Pushed bridge payload is stale or invalid for peer: ~S" name)
              (let* ((accepted-index ((self '~config-get) `(private bridge ,name last-index)))
                     (stage (sync-eval ((self '~field!) 'stage) #f)))
                ((stage 'set!) '(*transition* operation)
                 `((function synchronize!) (path (*bridge* ,name)) (index ,index) (accepted-index ,accepted-index)))
                ((self '~field!) 'stage (stage))
                `((ok? #t)
                  (mode push)
                  (accepted-index ,accepted-index))))))))

  (define-method (bridge-synchronize! self name index response (direction 'pull) local-interface)
    ;; Prepare or apply bridge synchronization.
    ;;   Args:
    ;;     name (symbol): bridge/subscriber name.
    ;;     index (integer): request index when applying response.
    ;;     response (list): optional payload returned by `synchronize` or ack returned by `synchronize!`.
    ;;     direction (symbol): `pull` or `push`.
    ;;     local-interface (string): this journal's externally reachable interface for push requests.
    ;;   Returns:
    ;;     alist request when response is omitted, otherwise local apply result.
    (if (not response)
        (if (eq? direction 'push)
            (let* ((last-ack ((self '~config-get) `(private subscriber ,name last-ack-index)))
                   (index (if (integer? last-ack) last-ack -1)))
              `((interface ,((self '~config-get) `(private subscriber ,name interface)))
                (index ,index)
                (query ((function synchronize!)
                        (arguments ((name ,((self '~config-get) `(private subscriber ,name remote-name)))
                                    (index ,index)
                                    (response ,((self 'synchronize) -1))
                                    (info ,((self 'info)))
                                    (interface ,local-interface)))))))
            (let ((index -1))
              `((interface ,((self '~config-get) `(private bridge ,name interface)))
                (index ,index)
                (query ((function synchronize) (arguments ((index ,index))))))))
        (if (and (list? response) (not (null? response)) (eq? (car response) 'error)) #f
            (if (eq? direction 'push)
                (let ((accepted (assoc 'accepted-index response)))
                  (if accepted ((self 'update-config!) `(private subscriber ,name last-ack-index) (cadr accepted)) #f))
                (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
                       (perm (sync-eval ((self '~field!) 'perm) #f))
                       (stage (sync-eval ((self '~field!) 'stage) #f))
                       (stage-set! (lambda (public-path storage-path value)
                                     ((stage 'copy!) '(*transition*) '(*transition* previous))
                                     ((stage 'set!) '(*transition* operation) `((path ,public-path) (value ,value)))
                                     ((stage 'set!) storage-path value))))
                  (if (eq? ((self '~config-get) `(private bridge ,name)) '()) #f
                      (let* ((init (> ((perm 'size)) 0))
                             (value (if init ((stage 'get) `(*bridge* ,name chain)) #f))
                             (last (if (sync-node? value) (sync-eval value #f) #f)))
                        (if (and last (> index 0) (< index (- ((last 'size)) 1))) #f
                            (let* ((latest ((standard 'deserialize) response))
                                   (latest-obj (sync-eval latest #f))
                                   (interface ((self '~config-get) `(private bridge ,name interface)))
                                   (public-key ((self '~config-get) `(private bridge ,name public-key)))
                                   (valid? (if (< index 0) #t
                                               (begin ((self '~signature-verify) latest public-key)
                                                      (if (and last (> ((last 'size)) 0))
                                                          (equal? ((last 'digest)) ((latest-obj 'digest) index)) #t))))
                                   (stored-chain (if (> ((latest-obj 'size)) 0)
                                                     ((standard 'deep-slice!) latest '(-1 ()))
                                                     latest))
                                   (stored-index (let ((chain (sync-eval stored-chain #f)))
                                                   (if (> ((chain 'size)) 0) (- ((chain 'size)) 1) -1)))
                                   (info `((valid? ,valid?)
                                           (index ,stored-index)
                                           (interface ,interface)
                                           (public-key ,public-key))))
                              ((self '~config-set!) `(private bridge ,name last-index) stored-index)
                              (stage-set! `(*bridge* ,name) `(*bridge* ,name info) info)
                              (stage-set! `(*bridge* ,name chain) `(*bridge* ,name chain) stored-chain)
                              ((self '~field!) 'stage (stage)))))))))))

  (define-method (step! self unix-time public-key secret-key)
    ;; Commit staged changes to permanent chain and update temp window.
    ;;   Args:
    ;;     unix-time (integer): step time as unix epoch seconds.
    ;;     public-key (byte-vector): public verification key derived for this step.
    ;;     secret-key (byte-vector): private signing key derived for this step.
    ;;   Returns:
    ;;     integer: new chain size.
    (let* ((window ((self '~config-get) '(public window)))
           (standard (sync-eval ((self '~field!) 'standard) #f))
           (stage (sync-eval ((self '~field!) 'stage) #f))
           (perm (sync-eval ((self '~field!) 'perm) #f))
           (temp (sync-eval ((self '~field!) 'temp) #f))
           (curr-digest (sync-digest
                         ((standard 'deep-call!) (stage) '()
                          '(lambda (obj)
                             ((obj 'set!) '(*transition*) '(nothing))))))
           (prev-digest (if (= ((perm 'size)) 0) (sync-digest (sync-null))
                            (sync-digest
                             ((standard 'deep-call!) ((perm 'get) -1) '()
                              '(lambda (obj)
                                 ((obj 'set!) '(*transition*) '(nothing))
                                 ((obj 'set!) '(*crypto*) '(nothing))))))))
      (if (not (equal? curr-digest prev-digest))
          (let ((utc-time (system-time-utc unix-time)))
            ((stage 'copy!) '(*transition*) '(*transition* previous))
            ((stage 'set!) '(*transition* operation) `((path (*state* *time*)) (value ,utc-time)))
            ((stage 'set!) '(*state* *time*) utc-time)
            ((perm 'push!) (stage))
            ((stage 'set!) '(*transition*) '(nothing))
            ((self '~config-set!) '(public public-key) public-key)
            (set! perm (sync-eval ((self '~signature-sign!) (perm) public-key secret-key) #f))
            ((temp 'push!) ((perm 'get) -1))
            (let ((time-node ((standard 'deep-slice!) (temp) '(-1 (*state* *time*)))))
              (if (and window (> ((temp 'size)) window)) ((temp 'prune!) (- (+ window 1))))
              ((self '~field!) 'stage (stage))
              ((self '~field!) 'perm
               ((standard 'deep-merge!) time-node ((standard 'deep-prune!) (perm) '(-1 (*state*)))))
              ((self '~field!) 'temp (temp)))))
      ((perm 'size))))

  (define-method (update-config! self path value)
    ;; Update a config entry in place.
    ;;   Args:
    ;;     path (list): config path to update.
    ;;     value (any): replacement value.
    ;;   Returns:
    ;;     boolean: #t after updating config.
    (if (equal? path '(public window))
        (let* ((temp (sync-eval ((self '~field!) 'temp) #f))
               (old-window ((self '~config-get) '(public window)))
               (size ((temp 'size))))
          (if (and (integer? value) (> size value))
              (let loop ((i (if (and (integer? old-window) (< value old-window))
                                (max 0 (- size old-window))
                                (+ size 1))))
                (if (> i (- size value 1)) #t
                    (begin ((temp 'prune!) i)
                           (loop (+ i 1))))))
          ((self '~field!) 'temp (temp))))
    ((self '~config-set!) path value))

  (define-method (update-code! self class (update '(lambda (obj) obj)))
    ;; Update one surface-level code object in place.
    ;;   Args:
    ;;     class (symbol): update target (`standard`, `tree`, `chain`, or `document`).
    ;;     update (expr): quoted function of form '(lambda (obj) ... obj)
    ;;   Returns:
    ;;     boolean: #t after updating code.
    (let ((update-func (eval update)))
      (case class
        ((standard) ((self '~field!) 'standard (update-func ((self '~field!) 'standard))))
        ((tree) ((self '~field!) 'stage (update-func ((self '~field!) 'stage))))
        ((chain) ((self '~field!) 'perm (update-func ((self '~field!) 'perm)))
         ((self '~field!) 'temp (update-func ((self '~field!) 'temp))))
        ((document) ((self '~field!) 'document (update-func ((self '~field!) 'document))))
        (else (error 'argument-error "Unrecognized class update candidate: ~S" class)))))

  (define-method (~document-field self meta?)
    ;; Select the internal document field for a public read option.
    ;;   Args:
    ;;     meta? (boolean): whether metadata was requested.
    ;;   Returns:
    ;;     symbol: `meta` when metadata requested, otherwise `value`.
    (if meta? 'meta 'value))

  (define-method (~document-new self value meta)
    ;; Build a new document object node.
    ;;   Args:
    ;;     value: document content.
    ;;     meta (alist): initial metadata dictionary or patch.
    ;;   Returns:
    ;;     sync node: document object node.
    (let ((document (sync-eval ((self '~field!) 'document) #f)))
      ((document '*init*) value (if (equal? meta '(nothing)) '() meta))
      (document)))

  (define-method (~object-type self node)
    ;; Return a standard object's declared type when available.
    ;;   Args:
    ;;     node: candidate object node.
    ;;   Returns:
    ;;     symbol/#f: object type, or #f when unavailable.
    (if (not (and (sync-node? node) (sync-pair? node) (byte-vector? (sync-car node)))) #f
        (let ((object (sync-eval node #f)))
          (if (memq '*type* (object '*api*)) ((object '*type*)) #f))))

  (define-method (~bridge-chain-value-path? self path)
    ;; Return whether a public path points at bridge chain storage rather than a document.
    ;;   Args:
    ;;     path (list): stage or indexed path.
    ;;   Returns:
    ;;     boolean: #t for bridge chain storage paths.
    (let ((segment (cond ((null? path) #f)
                         ((and (= (length path) 1) (pair? (car path))) (car path))
                         ((and (= (length path) 2) (integer? (car path)) (pair? (cadr path))) (cadr path))
                         (else #f))))
      (and segment
           (= (length segment) 3)
           (eq? (car segment) '*bridge*)
           (eq? (caddr segment) 'chain))))

  (define-method (~document-decode self value expression?)
    ;; Apply the public document read codec.
    ;;   Args:
    ;;     value: byte-vector payload or sentinel.
    ;;     expression? (boolean): if #t, decode bytes as an expression.
    ;;   Returns:
    ;;     any: byte-vector, decoded expression, or sentinel.
    (cond ((or (equal? value '(unknown)) (not expression?)) value)
          ((byte-vector? value) (byte-vector->expression value))
          (else (error 'value-error "Document payload cannot be decoded as an expression: ~S" value))))

  (define-method (~document-encode self value expression?)
    ;; Apply the public document write codec.
    ;;   Args:
    ;;     value: byte-vector payload, expression payload, or `(nothing)`.
    ;;     expression? (boolean): if #t, encode value as an expression.
    ;;   Returns:
    ;;     byte-vector or `(nothing)`.
    (cond ((equal? value '(nothing)) value)
          (expression? (expression->byte-vector value))
          ((byte-vector? value) value)
          (else (error 'value-error "Document value must be a byte-vector unless expression? is #t: ~S" value))))

  (define-method (~document-read self node meta? expression?)
    ;; Decode a document node for public reads.
    ;;   Args:
    ;;     node: tree value or sentinel.
    ;;     meta? (boolean): whether metadata was requested.
    ;;     expression? (boolean): if #t, decode payload bytes as an expression.
    ;;   Returns:
    ;;     any: document value, metadata envelope, or original non-document value.
    (cond ((eq? ((self '~object-type) node) 'document)
           (let* ((document (sync-eval node #f))
                  (value ((self '~document-decode) ((document 'get) 'value) expression?)))
             (if meta?
                 `((content ,value)
                   (meta ,((document 'get) 'meta)))
                 value)))
          ((and (list? node) (not (null? node)) (eq? (car node) 'directory))
           `(directory ,(map (lambda (entry)
                               `(,(car entry) ,(if (eq? (cadr entry) 'object) 'value (cadr entry))))
                             (cadr node))
                       ,(caddr node)))
          (else node)))

  (define-method (~document-write self existing value meta expression?)
    ;; Build an updated document node from an existing tree value.
    ;;   Args:
    ;;     existing: current tree value or sentinel.
    ;;     value: replacement document value, expression value, or `(nothing)`.
    ;;     meta (alist): metadata patch, `()`, or `(nothing)`.
    ;;     expression? (boolean): if #t, encode value as an expression.
    ;;   Returns:
    ;;     sync node or `(nothing)`: updated document object node or delete sentinel.
    (if (equal? value '(nothing)) '(nothing)
        (let* ((bytes ((self '~document-encode) value expression?))
               (type ((self '~object-type) existing))
               (document (cond ((eq? type 'document) (sync-eval existing #f))
                               ((equal? existing '(nothing)) (sync-eval ((self '~document-new) bytes '()) #f))
                               ((equal? existing '(unknown)) (error 'value-error "Cannot write document over unknown state"))
                               (else (error 'value-error "Cannot replace raw non-document state with a document: ~S" existing)))))
          ((document 'set!) 'value bytes)
          ((document 'set!) 'meta meta)
          (document))))

  (define-method (~field! self name value)
    ;; Resolve or set internal field by name.
    ;;   Args:
    ;;     name (symbol): field name.
    ;;     value (optional procedure returning value or #f): thunk to store, or #f to read.
    ;;   Returns:
    ;;     any: field value when value is #f, otherwise #t after set.
    (let ((address (case name
                     ((standard) '(1 0 0 0))
                     ((config) '(1 0 0 1))
                     ((stage) '(1 0 1 0))
                     ((temp) '(1 0 1 1))
                     ((document) '(1 1 0))
                     ((perm) '(1 1 1))
                     (else (error 'field-error "Ledger field not found: ~S" name)))))
      (if value (set! (self address) value)
          (self address))))

  (define-method (~path-normalize self path stage? state-only?)
    ;; Convert a flat public path into the current nested ledger representation.
    ;;   Args:
    ;;     path (list): flat public path segments.
    ;;     stage? (boolean): whether this is a staged-state path.
    ;;     state-only? (boolean): if #t, restrict to *state* paths only.
    ;;   Returns:
    ;;     list: internal nested path.
    (define (reject reason)
      (error 'path-error "Invalid ledger path (~A): ~S" reason path))
    (define (contains-pair? xs)
      (and (pair? xs)
           (or (pair? (car xs)) (contains-pair? (cdr xs)))))
    (define (indexed-tail segments)
      (cond ((null? segments) '())
            ((integer? (car segments)) (indexed-segments (car segments) (cdr segments)))
            (else (indexed-segments -1 segments))))
    (define (indexed-segments index segments)
      (cond ((null? segments) `(,index))
            ((eq? (car segments) '*state*) `(,index ,segments))
            ((eq? (car segments) '*transition*) `(,index ,segments))
            ((eq? (car segments) '*crypto*) `(,index ,segments))
            ((eq? (car segments) '*bridge*)
             (cond ((null? (cdr segments)) `(,index (*bridge*)))
                   ((null? (cddr segments)) `(,index (*bridge* ,(cadr segments) info)))
                   (else (append `(,index (*bridge* ,(cadr segments) chain))
                                 (indexed-tail (cddr segments))))))
            (else (reject "expected namespace marker"))))
    (cond ((not (list? path)) (reject "not a list"))
          ((contains-pair? path) (reject "nested public paths are not supported"))
          (stage?
           (cond ((or (null? path) (integer? (car path))) (reject "stage paths cannot start with an index"))
                 ((and state-only? (not (eq? (car path) '*state*))) (reject "expected *state*"))
                 ((eq? (car path) '*state*) `(,path))
                 ((and (not state-only?) (eq? (car path) '*transition*)) `(,path))
                 ((and (not state-only?) (eq? (car path) '*bridge*))
                  (cond ((null? (cdr path)) '((*bridge*)))
                        ((null? (cddr path)) `((*bridge* ,(cadr path) info)))
                        (else (reject "stage bridge traversal is not supported"))))
                 (else (reject "expected namespace marker"))))
          (else (indexed-tail path))))

  (define-method (~bridge-mode self publisher-policy subscriber-policy)
    ;; Determine bridge mode from publisher publish policy and subscriber subscribe policy.
    (let ((publish (cadr (assoc 'publish publisher-policy)))
          (subscribe (cadr (assoc 'subscribe subscriber-policy))))
      (cond ((or (eq? publish 'none) (eq? subscribe 'none)) 'none)
            ((eq? publish 'push)
             (if (memq subscribe '(push pull)) 'push
                 (error 'bridge-mode-error "Invalid subscribe policy: ~S" subscribe)))
            ((eq? publish 'pull)
             (cond ((eq? subscribe 'pull) 'pull)
                   ((eq? subscribe 'push)
                    (error 'bridge-mode-error "Incompatible bridge policies: publisher=~S subscriber=~S" publish subscribe))
                   (else (error 'bridge-mode-error "Invalid subscribe policy: ~S" subscribe))))
            (else (error 'bridge-mode-error "Invalid publish policy: ~S" publish)))))

  (define-method (~delete-bridge! self name)
    ;; Remove current bridge config and staged bridge exposure while preserving history.
    (let ((stage (sync-eval ((self '~field!) 'stage) #f)))
      ((self '~config-set!) `(private bridge ,name) '())
      ((stage 'set!) `(*bridge* ,name) '(nothing))
      ((self '~field!) 'stage (stage))))

  (define-method (~config-get self (path '()))
    (let loop ((config (byte-vector->expression ((self '~field!) 'config))) (path path))
      (if (null? path) config
          (let ((match (assoc (car path) config)))
            (if (not match) '()
                (loop (cadr match) (cdr path)))))))

  (define-method (~config-set! self (path '()) value)
    ((self '~field!) 'config
     (expression->byte-vector
      (let loop-1 ((config (byte-vector->expression ((self '~field!) 'config))) (path path))
        (if (null? path) value
            (let loop-2 ((config config))
              (cond ((null? config)
                     (if (eq? value '()) '()
                         (list (list (car path) (loop-1 '() (cdr path))))))
                    ((eq? (caar config) (car path))
                     (let ((result (loop-1 (cadar config) (cdr path))))
                       (if (eq? result '()) (cdr config)
                           (cons (list (car path) result) (cdr config)))))
                    (else (cons (car config) (loop-2 (cdr config)))))))))))

  (define-method (~head self path (index -1))
    ;; Fetch the appropriate local chain head node for a path/index.
    ;;   Args:
    ;;     path (list): path segments.
    ;;     index (integer): chain index to use.
    ;;   Returns:
    ;;     sync node: chain head node.
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (perm (sync-eval ((self '~field!) 'perm) #f))
           (window ((self '~config-get) '(public window)))
           (target ((perm 'index) (cond ((null? path) -1) ((>= (car path) 0) (car path)) (else (+ index 1 (car path))))))
           (current (- ((perm 'size)) 1)))
      (let ((chain (if (and window (<= target (- current window))) perm (sync-eval ((self '~field!) 'temp) #f))))
        ((chain 'previous) index))))

  (define-method (~signature-sign! self chain public-key secret-key)
    ;; Embed public key and signature into chain head using ephemeral step keys.
    ;;   Args:
    ;;     chain (sync node): chain node to sign.
    ;;     public-key (byte-vector): public verification key.
    ;;     secret-key (byte-vector): private signing key for this step only.
    ;;   Returns:
    ;;     sync node: signed chain node.
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f)))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* public-key)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* signature)) #u()))
      ((standard 'deep-call!) chain '(-1)
       `(lambda (tree)
          ((tree 'set!) '(*crypto* public-key) ,public-key)
          ((tree 'set!) '(*crypto* signature) (crypto-sign ,secret-key ,(sync-digest chain)))))))

  (define-method (~signature-verify self chain public-key)
    ;; Verify chain head signature and optional expected public key.
    ;;   Args:
    ;;     chain (sync node): chain node to verify.
    ;;     public-key (byte-vector or #f): expected public key.
    ;;   Returns:
    ;;     boolean: #t when signature verifies (or raises on failure).
    (let* ((standard (sync-eval ((self '~field!) 'standard) #f))
           (chain-copy chain))
      (set! chain-copy ((standard 'deep-set!) chain-copy '(-1 (*crypto* public-key)) #u()))
      (set! chain-copy ((standard 'deep-set!) chain-copy '(-1 (*crypto* signature)) #u()))
      (let* ((included-key ((standard 'deep-call) chain '(-1)
                            '(lambda (tree)
                               ((tree 'get) '(*crypto* public-key)))))
             (signature ((standard 'deep-call) chain '(-1)
                         '(lambda (tree)
                            ((tree 'get) '(*crypto* signature))))))
        (cond ((or (null? signature) (null? included-key))
               (error 'integrity-error "Chain does not include public key and signature"))
              ((and public-key (not (equal? public-key included-key)))
               (error 'integrity-error "Included public key does not match expected key"))
              ((not (crypto-verify (if public-key public-key included-key) signature (sync-digest chain-copy)))
               (error 'integrity-error "Included signature does not verify"))
              (else #t))))))
