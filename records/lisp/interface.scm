(macro (config standard chain tree ledger document . classes)

  (define (config-ref key) (cadr (assoc key config)))

  (for-each (lambda (key)
              (if (not (assoc key config))
                  (error 'argument-error "Missing interface config value: ~S" key)))
            '(root-secret interface-secret root))

  (if (or (not (string? (config-ref 'root-secret)))
          (not (string? (config-ref 'interface-secret))))
      (error 'argument-error "Interface secrets must be strings: ~S ~S"
             (config-ref 'root-secret) (config-ref 'interface-secret)))

  (set! config (append config `((clear? #t)
                                (admins ())
                                (window #f)
                                (interface ,(config-ref 'interface-secret))
                                (name ,(config-ref 'interface-secret))
                                (push-enabled? #f)
                                (bridge-policy ((publish push) (subscribe pull))))))

  ;; install root logic
  (if (config-ref 'clear?)
      (sync-call `(,(config-ref 'root) ,(config-ref 'root-secret) ,(config-ref 'clear?)) #t)
      (sync-call `(*eval* ,(config-ref 'root-secret)
                          (,(config-ref 'root) ,(config-ref 'root-secret) ,(config-ref 'clear?))) #t))

  (define (call function)
    (sync-call `(*call* ,(config-ref 'root-secret) ,function) #t))

  (define (set-query function)
    (sync-call `(*set-query* ,(config-ref 'root-secret) ,function) #t))

  (define (set-step function)
    (sync-call `(*set-step* ,(config-ref 'root-secret) ,function) #t))

  ;; install and instantiate standard library
  (call `(lambda (root)
           (let ((init (caddr ,standard)))
             ((root 'set!) '(root class standard) ,standard)
             ((root 'set!) '(root object standard)
              ((eval `(lambda* ,(cddadr init) ,@(cddr init))) ,standard)))))

  ;; install required classes
  (call `(lambda (root) ((root 'set!) '(root class chain) ,chain)))
  (call `(lambda (root) ((root 'set!) '(root class tree) ,tree)))
  (call `(lambda (root) ((root 'set!) '(root class ledger) ,ledger)))
  (call `(lambda (root) ((root 'set!) '(root class document) ,document)))

  ;; install optional classes
  (let loop ((classes classes))
    (if (null? classes) #t
        (begin (call `(lambda (root ((root 'set!) '(root object ,(caar classes)) ,(cadadr classes)))))
               (loop (cdr classes)))))

  (call `(lambda (root)
           (let* ((std-node ((root 'get) '(root object standard)))
                  (standard (sync-eval std-node #f))
                  (standard-class ((root 'get) '(root class standard)))
                  (tree-class ((root 'get) '(root class tree)))
                  (chain-class ((root 'get) '(root class chain)))
                  (ledger-class ((root 'get) '(root class ledger)))
                  (document-class ((root 'get) '(root class document)))
                 (keys (crypto-generate (expression->byte-vector ,(config-ref 'root-secret)))))
             (if ,(config-ref 'clear?)
                 (let* ((config-expr (list (list 'public (list (list 'window ,(config-ref 'window))
                                                               (list 'public-key (car keys))
                                                               (list 'bridge-policy ',(config-ref 'bridge-policy))))
                                           (list 'private '())))
                        (ledger ((standard 'init) ledger-class std-node config-expr tree-class chain-class document-class)))
                   ((root 'set!) '(root object ledger) ledger))
                 (let* ((ledger-old (sync-eval ((root 'get) '(root object ledger)) #f))
                        (ledger (sync-eval (sync-cons (sync-car ((standard 'make) ledger-class)) (ledger-old '(1))) #f))
                        (recode (lambda (class)
                                  (let ((code (sync-car ((standard 'make) class))))
                                    `(lambda (obj)
                                       (sync-cons ,code (sync-cdr obj)))))))
                   ((ledger 'update-config!) '(public window) ,(config-ref 'window))
                   ((ledger 'update-config!) '(public public-key) (car keys))
                   ((ledger 'update-config!) '(public bridge-policy) ',(config-ref 'bridge-policy))
                   ((ledger 'update-code!) 'standard (recode standard-class))
                   ((ledger 'update-code!) 'tree (recode tree-class))
                   ((ledger 'update-code!) 'chain (recode chain-class))
                   ((ledger 'update-code!) 'document (recode document-class))
                   ((root 'set!) '(root object ledger) (ledger)))))))

  ;; define secret store and admin list
  (call `(lambda (root)
           ((root 'set!) '(interface secret) (sync-hash (expression->byte-vector ,(config-ref 'interface-secret))))
           ((root 'set!) '(interface admins) ,(config-ref 'admins))
           ((root 'set!) '(interface endpoint) ,(config-ref 'interface))
           ((root 'set!) '(interface name) ,(config-ref 'name))
           ((root 'set!) '(interface push-enabled?) ,(config-ref 'push-enabled?))))

  (define query-once
    '(lambda (query)
       (let* ((func (assoc 'function query))
              (args (assoc 'arguments query))
              (auth (assoc 'authentication query))
              (arg-list (if args (cadr args) '()))
              (keyword-args (let loop ((in (reverse arg-list)) (out '()))
                              (if (null? in) out
                                  (loop (cdr in) (append `(,(symbol->keyword (caar in)) ,(cadar in)) out)))))
              (std-node ((root 'get) '(root object standard)))
              (standard (sync-eval std-node #f))
              (node ((root 'get) '(root object ledger)))
              (ledger (sync-eval node #f)))

         ;; --- query helpers ---

         (define (~with-auth query)
           (append query `((authentication ,(cadr auth)))))

         (define (~self-call function args blocking?)
           (sync-call (~with-auth `((function ,function) (arguments ,args))) blocking?))

         (define (~authenticate+authorize)
           (let* ((auth-val (cadr auth))
                  (identity (if (assoc 'identity auth-val) (cadr (assoc 'identity auth-val)) '*journal*))
                  (credentials (cadr (assoc 'credentials auth-val)))
                  (admins ((root 'get) '(interface admins)))
                  (admin? (or (eq? identity '*journal*) (member identity admins))))
             (if (not (equal? (sync-hash (expression->byte-vector credentials))
                              ((root 'get) '(interface secret))))
                 (error 'authentication-error "Could not authenticate restricted interface call for identity: ~S" identity))
             (if (not admin?)
                 (let ((segment (lambda (path)
                                  (let ((segments (if (and (pair? path) (integer? (car path))) (cdr path) path)))
                                    (if (and (pair? segments) (memq (car segments) '(*state* *transition*)))
                                        (cdr segments)
                                        '())))))
                   (case (cadr func)
                     ((set!)
                      (if (not (eq? identity (car (segment (cadr (assoc 'path arg-list))))))
                          (error 'authorization-error "User may only write to their own space: ~S" identity)))
                     ((set-batch!)
                      (for-each (lambda (path)
                                  (if (not (eq? identity (car (segment path))))
                                      (error 'authorization-error "User may only write to their own space: ~S" identity)))
                                (cadr (assoc 'paths arg-list))))
                     ((get resolve pin! unpin!)
                      (let ((seg (segment (cadr (assoc 'path arg-list)))))
                        (if (not (or (null? seg) (eq? identity (car seg)) (not (memq '*private* seg))))
                            (error 'authorization-error "User may not read another user's private namespace: ~S" identity))))
                     (else (error 'authorization-error "Operation requires admin privileges: ~S" (cadr func))))))))

         (define (~bridge-path? path)
           (let ((segments (if (and (pair? path) (integer? (car path))) (cdr path) path)))
             (and (pair? segments) (eq? (car segments) '*bridge*))))

         ;; --- remote bridge helpers ---

         (define* (~fetch-remote-head path index (meta? #f))
           (let* ((request ((ledger 'bridge-head) path index))
                  (interface (cadr (assoc 'interface request)))
                  (remote-index (cadr (assoc 'index request)))
                  (remote-path (cadr (assoc 'path request)))
                  (query `((function trace) (arguments ((index ,remote-index) (path ,remote-path) (meta? ,meta?)))))
                  (response (sync-remote interface query)))
             ((standard 'deserialize) response)))

         (define* (~fetch-merged-head path (index -1) (meta? #f))
           (let ((head (~fetch-remote-head path index meta?)))
             ((ledger 'merge-head) path head index)))

         ;; --- handlers ---

         (define* (*secret* (secret (error 'argument-error "Missing required argument: ~S" 'secret)))
           ;; Set the interface authentication secret.
           ;;   Args:
           ;;     secret (string): new interface secret.
           ;;   Returns:
           ;;     sync hash: stored secret hash.
           ((root 'set!) '(interface secret) (sync-hash (expression->byte-vector secret))))

         (define (*admins-get*)
           ;; Return the current admin username list.
           ((root 'get) '(interface admins)))

         (define* (*admins-set* (admins (error 'argument-error "Missing required argument: ~S" 'admins)))
           ;; Replace the admin username list wholesale.
           ;;   Args:
           ;;     admins (list of strings): new admin usernames.
           ;;   Returns:
           ;;     stored value.
           ((root 'set!) '(interface admins) admins))

         (define* (*window-set* (value (error 'argument-error "Missing required argument: ~S" 'value)))
           ;; Update the public retention window.
           ;;   Args:
           ;;     value (integer): positive retention window size.
           ;;   Returns:
           ;;     boolean: #t after updating.
           (if (not (and (integer? value) (> value 0)))
               (error 'argument-error "Window must be a positive integer: ~S" value))
           ((ledger 'update-config!) '(public window) value))

         (define* (config (path '()))
           ;; Return ledger configuration through the admin-only envelope.
           ;;   Args:
           ;;     path (list): optional config path.
           ;;   Returns:
           ;;     any: config expression or subexpression.
           ((ledger 'config) path))

         (define* (get (path (error 'argument-error "Missing required argument: ~S" 'path)) meta? expression?)
           ;; Get staged document value, optionally with metadata and expression decoding.
           ;;   Args:
           ;;     path (list): target path.
           ;;     meta? (boolean): #t to return value and metadata envelope.
           ;;     expression? (boolean): #t to decode document payload bytes as an expression.
           ;;   Returns:
           ;;     any: staged byte-vector/expression value, metadata envelope, sentinel, or directory listing.
           ((ledger 'get) path meta? expression?))

         (define* (set-document! (path (error 'argument-error "Missing required argument: ~S" 'path)) value meta expression?)
           ;; Stage a document content and/or metadata update.
           ;;   Args:
           ;;     path (list): target path.
           ;;     value: optional byte-vector payload, expression payload when expression? is #t, or `(nothing)`.
           ;;     meta: optional metadata patch.
           ;;     expression? (boolean): #t to encode value as an expression before storing bytes.
           ;;   Returns:
           ;;     boolean: #t after staging.
           (let ((value-entry (assoc 'value arg-list))
                 (meta-entry (assoc 'meta arg-list)))
             (if (and (not value-entry) (not meta-entry))
                 (error 'argument-error "set! requires value or metadata for path: ~S" path))
             (let* ((meta (if meta-entry (cadr meta-entry) '()))
                    (value (if value-entry (cadr value-entry)
                               (let ((current ((ledger 'get) path)))
                                 (if (or (equal? current '(nothing))
                                         (equal? current '(unknown))
                                         (and (list? current) (not (null? current)) (eq? (car current) 'directory)))
                                     (error 'value-error "Metadata-only writes require an existing document at path: ~S" path)
                                     current)))))
               ((ledger 'set!) path value meta expression?))))

         (define* (resolve (path (error 'argument-error "Missing required argument: ~S" 'path)) pinned? proof? head meta? expression?)
           ;; Resolve a path, optionally using a provided proof head or remote bridge fetch.
           ;;   Args:
           ;;     path (list): target path.
           ;;     pinned? (boolean): #t to prefer pinned history.
           ;;     proof? (boolean): #t to return proof-ish result.
           ;;     head (sync node): optional pre-fetched chain head.
           ;;     meta? (boolean): #t to return value and metadata envelope.
           ;;     expression? (boolean): #t to decode document payload bytes as an expression.
           ;;   Returns:
           ;;     any: resolved byte-vector/expression value, metadata envelope, or sentinel.
           (if head ((ledger 'resolve) path pinned? proof? head meta? expression?)
               (let ((attempt ((ledger 'resolve) path #f #f #f meta? expression?)))
                 (cond ((not (equal? attempt '(unknown))) ((ledger 'resolve) path pinned? proof? #f meta? expression?))
                       ((not (~bridge-path? path)) attempt)
                       (else ((ledger 'resolve) path pinned? proof? (~fetch-merged-head path -1 meta?) meta? expression?))))))

         (define* (trace (index (error 'argument-error "Missing required argument: ~S" 'index)) (path (error 'argument-error "Missing required argument: ~S" 'path)) head meta?)
           ;; Return a proof trace for a path, fetching remote bridge state when needed.
           ;;   Args:
           ;;     index (integer): trace index.
           ;;     path (list): target path.
           ;;     head (sync node): optional pre-fetched chain head.
           ;;   Returns:
           ;;     sync node: traced proof.
           (if head ((ledger 'trace) index path head meta?)
               (let ((attempt ((ledger 'resolve) (cond ((null? path) `(,index))
                                                       ((not (integer? (car path)))
                                                        (if (>= index 0) (cons index path) path))
                                                       ((>= (car path) 0) path)
                                                       ((>= index 0) (cons (+ (+ index 1) (car path)) (cdr path)))
                                                       (else (cons (+ index (car path) 1) (cdr path)))))))
                 (cond ((not (equal? attempt '(unknown))) ((ledger 'trace) index path #f meta?))
                       ((not (~bridge-path? path)) (error 'path-error "Cannot trace unknown path: ~S" path))
                       (else ((ledger 'trace) index path (~fetch-merged-head path index meta?) meta?))))))

         (define* (pin! (path (error 'argument-error "Missing required argument: ~S" 'path)) response)
           ;; Pin content locally, fetching and reserializing remote bridge content when needed.
           ;;   Args:
           ;;     path (list): target path.
           ;;     response (expression): optional serialized pin response.
           ;;   Returns:
           ;;     boolean: #t after pinning.
           (if response ((ledger 'pin!) path response)
               (let ((attempt ((ledger 'resolve) path)))
                 (cond ((not (equal? attempt '(unknown))) ((ledger 'pin!) path))
                       ((not (~bridge-path? path)) (error 'path-error "Cannot pin unknown content at path: ~S" path))
                       (else (let* ((merged (~fetch-merged-head path))
                                    (response ((ledger 'trace) -1 path merged))
                                    (args (append arg-list `((response ,response)))))
                               (~self-call 'pin! args #t)))))))

         (define* (bridge! (name (error 'argument-error "Missing required argument: ~S" 'name))
                           (info-local (error 'argument-error "Missing required argument: ~S" 'info-local))
                           info-remote)
           ;; Register a bridge/publication target, lazily fetching remote info when omitted.
           ;;   Args:
           ;;     name (symbol): local bridge/publication target name.
           ;;     info-local (alist): locally stored bridge info, including interface/policy/role/remote-name.
           ;;     info-remote (alist): optional remote public info payload.
           ;;   Returns:
           ;;     boolean: #t after bridge registration.
           (if info-remote ((ledger 'bridge!) name info-local info-remote)
               (let* ((info-remote (sync-remote (cadr (assoc 'interface info-local)) '((function info))))
                      (args `((name ,name) (info-local ,info-local) (info-remote ,info-remote))))
                 (~self-call 'bridge! args #t))))

         (define (~method)
           (let ((result (apply (ledger (cadr func)) keyword-args)))
             result))

         ;; --- dispatch ---

         (let ((ret (case (cadr func)
                      ((*secret*) (~authenticate+authorize) (apply *secret* keyword-args))
                      ((*admins-get*) (~authenticate+authorize) (apply *admins-get* keyword-args))
                      ((*admins-set*) (~authenticate+authorize) (apply *admins-set* keyword-args))
                      ((*window-set*) (~authenticate+authorize) (apply *window-set* keyword-args))
                      ((get) (~authenticate+authorize) (apply get keyword-args))
                      ((set!) (~authenticate+authorize) (apply set-document! keyword-args))
                      ((resolve) (~authenticate+authorize) (apply resolve keyword-args))
                      ((trace) (apply trace keyword-args))
                      ((pin!) (~authenticate+authorize) (apply pin! keyword-args))
                      ((bridge!) (~authenticate+authorize) (apply bridge! keyword-args))
                      ((config) (~authenticate+authorize) (apply config keyword-args))
                      ((set-batch! unpin!) (~authenticate+authorize) (~method))
                      ((size synchronize synchronize! info) (~method))
                      (else (error 'api-error "Interface does not implement API endpoint: ~S" (cadr func))))))
           ((root 'set!) '(root object ledger) (ledger))
           ret))))

  (set-query
   `(lambda (root query)
      (let ((query-once ,query-once))
        (if (not (eq? (cadr (assoc 'function query)) 'batch!)) (query-once query)
            (let ((auth (assoc 'authentication query)))
              (let loop ((queries (cadr (assoc 'queries (cadr (assoc 'arguments query))))) (result '()))
                (if (null? queries) (reverse result)
                    (let ((subquery (if auth (append (car queries) (list auth)) (car queries))))
                      (loop (cdr queries) (cons (query-once subquery) result))))))))))
  (define step-once
    '(lambda (root secret query)
       (let* ((query (if (null? query) '(ledger-step) query))
              (std-node ((root 'get) '(root object standard)))
              (standard (sync-eval std-node #f))
              (node ((root 'get) '(root object ledger)))
              (ledger (sync-eval node #f)))

         ;; --- query helpers ---

         (define (~self-call blocking? query)
           (sync-call `(*step* ,secret ,query) blocking?))

         ;; --- handlers ---

         (define* (bridge-synchronize! (name (error 'argument-error "Missing required argument: ~S" 'name)) direction index response)
           ;; Synchronize a bridge/subscriber by routing a ledger-prepared request.
           ;;   Args:
           ;;     name (symbol): bridge or subscriber name.
           ;;     direction (symbol): `pull` or `push`.
           ;;     index (integer): request index when applying response.
           ;;     response (expression): optional remote response/ack.
           ;;   Returns:
           ;;     boolean: #t/#f after synchronization or ack handling.
           (if response ((ledger 'bridge-synchronize!) name index response direction)
               (let* ((request ((ledger 'bridge-synchronize!) name #f #f direction ((root 'get) '(interface endpoint))))
                      (response (sync-remote (cadr (assoc 'interface request)) (cadr (assoc 'query request)))))
                 (~self-call #t `(bridge-synchronize! ,name ,direction ,(cadr (assoc 'index request)) ,response)))))

         (define* (ledger-step mutate?)
           ;; Run one interface step, optionally mutating the local ledger at the end.
           ;;   Args:
           ;;     mutate? (boolean): #t to perform the final local step.
           ;;   Returns:
           ;;     integer: resulting ledger size.
           (if mutate? (let ((keys (crypto-generate (expression->byte-vector secret))))
                         ((ledger 'step!) (system-time-unix) (car keys) (cdr keys))
                         ((ledger 'size)))
               (begin
                 (let loop ((names (map car ((ledger 'config) '(private bridge)))))
                   (if (not (null? names))
                       (begin
                         (~self-call #f `(bridge-synchronize! ,(car names) pull))
                         (loop (cdr names)))))
                 (let ((size (~self-call #t '(ledger-step #t))))
                   (if ((root 'get) '(interface push-enabled?))
                       (let loop ((subscribers ((ledger 'config) '(private subscriber))))
                         (if (not (null? subscribers))
                             (begin
                               (if (eq? ((ledger 'config) `(private subscriber ,(caar subscribers) policy mode)) 'push)
                                   (~self-call #f `(bridge-synchronize! ,(caar subscribers) push)))
                               (loop (cdr subscribers))))))
                   size))))

         ;; --- dispatch ---

         (let ((ret (case (car query)
                      ((ledger-step) (apply ledger-step (cdr query)))
                      ((bridge-synchronize!) (apply bridge-synchronize! (cdr query)))
                      (else (error 'api-error "Step does not implement operation: ~S" (cadr func))))))
           ((root 'set!) '(root object ledger) (ledger))
           ret))))

  (set-step step-once)

  "Installed interface")
